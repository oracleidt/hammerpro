use egui::{Key, Modifiers, Pos2};
use glam::DVec3;
use hp_app::state::{PickKind, PickRequest, PickResult};
use hp_app::view::{ViewInfo, ViewKind};
use hp_app::{AppState, t, tf};
use hp_editor::{FaceRef, Id, ObjRef, Owner};
use hp_geom::ViewAxis;
use hp_render::Overlay;

use crate::{input, overlay, placement};

pub const PICK_COLOR: [f32; 4] = [0.25, 0.85, 1.0, 1.0];
const PICK_POINTS: f64 = 4.0;

pub fn pending(state: &AppState) -> Option<&PickRequest> {
    state.pick.as_ref().filter(|p| p.result.is_none())
}

fn finish(state: &mut AppState, result: PickResult) {
    if let Some(p) = state.pick.as_mut() {
        p.result = Some(result);
    }
}

pub fn hint(state: &AppState, collected: usize) -> Option<String> {
    let req = pending(state)?;
    Some(if req.kind == PickKind::Entity {
        t!("viewport.pick_entity").to_owned()
    } else if req.kind == PickKind::Faces {
        tf!("viewport.pick_faces", collected)
    } else if req.kind == PickKind::Angles {
        t!("viewport.pick_angles").to_owned()
    } else {
        t!("viewport.pick_point").to_owned()
    })
}

fn view_axis(kind: ViewKind) -> Option<ViewAxis> {
    match kind {
        ViewKind::Top => Some(ViewAxis::Top),
        ViewKind::Front => Some(ViewAxis::Front),
        ViewKind::Side => Some(ViewAxis::Side),
        _ => None,
    }
}

pub fn entity_at(state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<Id> {
    let scene = state.doc.scene();
    let entity_of = |obj: ObjRef| match obj {
        ObjRef::Entity(id) => Some(id),
        ObjRef::Solid(id) => match scene.solid(id)?.owner {
            Owner::Entity(e) => Some(e),
            Owner::World => None,
        },
    };
    match view_axis(view.kind) {
        Some(axis) => {
            let tol = PICK_POINTS * view.units_per_point();
            let picks = state.doc.pick_2d(axis, view.screen_to_2d(pos), tol, &|e| state.assets.entity_bounds(e));
            picks.into_iter().find_map(entity_of)
        }
        None => placement::pick_surface(state, view, pos).and_then(|hit| entity_of(hit.obj)),
    }
}

pub fn face_at(state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<FaceRef> {
    if view.is_2d() {
        return None;
    }
    let hit = placement::pick_surface(state, view, pos)?;
    match (hit.obj, hit.face) {
        (ObjRef::Solid(id), Some(side)) => Some(FaceRef::new(id, side)),
        _ => None,
    }
}

pub fn point_at(state: &AppState, view: &ViewInfo, pos: Pos2, depth_2d: f64) -> Option<DVec3> {
    if view.is_2d() {
        Some(placement::point_2d(state, view, pos, depth_2d))
    } else {
        placement::pick_surface(state, view, pos).map(|hit| hit.point).filter(|p| p.is_finite())
    }
}

const AIM_FAR: f64 = 16384.0;

pub fn aim_point_at(state: &AppState, view: &ViewInfo, pos: Pos2, depth_2d: f64) -> Option<DVec3> {
    let origin_of = |id: Id| state.doc.scene().entity(id).and_then(|n| n.entity.origin()).filter(|p| p.is_finite());
    if view.is_2d() {
        let p = placement::point_2d(state, view, pos, depth_2d);
        return Some(entity_at(state, view, pos).and_then(origin_of).unwrap_or(p));
    }
    match placement::pick_surface(state, view, pos) {
        Some(hit) => {
            let entity = match hit.obj {
                ObjRef::Entity(id) => origin_of(id),
                ObjRef::Solid(_) => None,
            };
            entity.or(Some(hit.point).filter(|p| p.is_finite()))
        }
        None => {
            let ray = placement::view_ray(view, pos);
            Some(ray.origin + ray.dir.normalize_or_zero() * AIM_FAR).filter(|p| p.is_finite())
        }
    }
}

#[derive(Debug, Default)]
pub struct Picker {
    owner: Option<String>,
    faces: Vec<FaceRef>,
}

impl Picker {
    pub fn faces(&self) -> &[FaceRef] {
        &self.faces
    }

    pub fn sync(&mut self, state: &AppState) {
        let current = pending(state).map(|p| p.requester.as_str());
        if self.owner.as_deref() != current {
            self.owner = current.map(str::to_owned);
            self.faces.clear();
        }
    }

    pub fn keys(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(req) = pending(state) else { return };
        if ctx.text_edit_focused() {
            return;
        }
        let faces = req.kind == PickKind::Faces;
        let (escape, enter) = ctx.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::Enter)));
        if escape {
            input::consume_escape(ctx);
            self.faces.clear();
            finish(state, PickResult::Cancelled);
        } else if enter && faces && !self.faces.is_empty() {
            ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
            let faces = std::mem::take(&mut self.faces);
            finish(state, PickResult::Faces(faces.into_iter().map(|f| (f.solid, f.side)).collect()));
        }
    }

    pub fn click(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2, modifiers: Modifiers, depth_2d: f64) {
        let Some(req) = pending(state) else { return };
        let kind = req.kind.clone();
        let result = if kind == PickKind::Entity {
            entity_at(state, view, pos).map(PickResult::Entity)
        } else if kind == PickKind::Faces {
            let Some(face) = face_at(state, view, pos) else { return };
            if modifiers.ctrl || modifiers.command {
                match self.faces.iter().position(|f| *f == face) {
                    Some(i) => {
                        self.faces.remove(i);
                    }
                    None => self.faces.push(face),
                }
                return;
            }
            let mut faces = std::mem::take(&mut self.faces);
            if !faces.contains(&face) {
                faces.push(face);
            }
            Some(PickResult::Faces(faces.into_iter().map(|f| (f.solid, f.side)).collect()))
        } else if kind == PickKind::Angles {
            aim_point_at(state, view, pos, depth_2d).map(PickResult::Point)
        } else if kind == PickKind::Point {
            point_at(state, view, pos, depth_2d).map(PickResult::Point)
        } else {
            None
        };
        if let Some(r) = result {
            finish(state, r);
        }
    }

    pub fn add_overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if self.faces.is_empty() || pending(state).is_none() {
            return;
        }
        for face in &self.faces {
            if let Some(poly) = state.doc.face_polygon(*face) {
                overlay::face_highlight(out, view, poly, PICK_COLOR);
            }
        }
    }
}
