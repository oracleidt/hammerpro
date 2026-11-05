use egui::{Align2, Color32, Stroke, vec2};
use glam::DVec3;
use hp_app::actions::shortcut_label;
use hp_app::runtime::FlyTextureOp;
use hp_app::view::ViewInfo;
use hp_app::{Action, AppState, Severity, t, tf};
use hp_editor::{FaceRef, ObjRef, PickOptions};
use hp_geom::Ray;
use hp_render::{Overlay, OverlayLine, OverlayTri};

use crate::input::{FrameInput, MIDDLE, SECONDARY};
use crate::layers::OverlayLayer;
use crate::paint;

pub const REACH: f64 = 16384.0;

pub const OPS: [(Action, FlyTextureOp); 4] = [
    (Action::FlyApplyTexture, FlyTextureOp::Apply),
    (Action::FlyApplyTextureBrush, FlyTextureOp::ApplyBrush),
    (Action::FlyPickTexture, FlyTextureOp::Pick),
    (Action::FlyAlignTexture, FlyTextureOp::Align),
];

pub fn op_of(action: Action) -> Option<FlyTextureOp> {
    OPS.iter().find(|(a, _)| *a == action).map(|(_, op)| *op)
}

pub fn target_face(state: &AppState) -> Option<FaceRef> {
    let cam = state.cameras.cam3d;
    let forward = cam.forward();
    if !forward.is_finite() || !cam.pos.is_finite() {
        return None;
    }
    let ray = Ray::new(cam.pos, forward);
    let opts = PickOptions { include_entities: false, ..PickOptions::default() };
    let hit = state.view_pick(&ray, &opts)?;
    if hit.t > REACH {
        return None;
    }
    match (hit.obj, hit.face) {
        (ObjRef::Solid(id), Some(side)) => Some(FaceRef::new(id, side)),
        _ => None,
    }
}

pub fn refresh_target(state: &mut AppState) {
    let cam = state.cameras.cam3d;
    let key = (cam.pos, cam.yaw_deg, cam.pitch_deg, state.doc.revision());
    if state.runtime.lab.fly_texture.key == Some(key) {
        return;
    }
    let face = target_face(state);
    let material =
        face.and_then(|f| state.doc.face_side(f).map(|s| s.material.to_ascii_lowercase())).unwrap_or_default();
    let fly = &mut state.runtime.lab.fly_texture;
    fly.key = Some(key);
    fly.target = face;
    fly.material = material;
}

pub fn enter(state: &mut AppState, view: usize) {
    let fly = &mut state.runtime.lab.fly_texture;
    fly.active = true;
    fly.view = view;
    fly.key = None;
    refresh_target(state);
}

pub fn leave(state: &mut AppState) {
    let fly = &mut state.runtime.lab.fly_texture;
    if fly.active || fly.target.is_some() {
        fly.active = false;
        fly.target = None;
        fly.key = None;
        fly.material.clear();
    }
}

pub fn run(state: &mut AppState, op: FlyTextureOp) -> bool {
    state.runtime.lab.fly_texture.key = None;
    refresh_target(state);
    let Some(face) = state.runtime.lab.fly_texture.target else {
        state.notify(Severity::Info, t!("viewport.fly_tex.nothing"));
        return false;
    };
    let done = match op {
        FlyTextureOp::Apply => apply(state, &[face]),
        FlyTextureOp::ApplyBrush => {
            let faces = state.doc.all_faces_of(&[ObjRef::Solid(face.solid)]);
            apply(state, &faces)
        }
        FlyTextureOp::Pick => match state.doc.face_side(face).map(|s| s.material.clone()) {
            Some(name) if !name.trim().is_empty() => {
                state.use_material(name.trim());
                let shown = state.current_material.clone();
                state.toasts.push_keyed("fly_pick", Severity::Info, tf!("viewport.fly_tex.picked", shown));
                true
            }
            _ => false,
        },
        FlyTextureOp::Align => {
            let cam = state.cameras.cam3d;
            match state.doc.align_faces_to_view(&[face], cam.right(), cam.up()) {
                Ok(_) => true,
                Err(e) => {
                    state.notify(Severity::Error, tf!("viewport.fly_tex.failed", e));
                    false
                }
            }
        }
    };
    if done {
        state.runtime.lab.fly_texture.last = Some(op);
        state.runtime.lab.fly_texture.key = None;
        refresh_target(state);
    }
    done
}

fn apply(state: &mut AppState, faces: &[FaceRef]) -> bool {
    let material = state.current_material.clone();
    match state.doc.apply_material(faces, &material) {
        Ok(_) => {
            state.use_material(&material);
            true
        }
        Err(e) => {
            state.notify(Severity::Error, tf!("viewport.fly_tex.failed", e));
            false
        }
    }
}

pub fn frame(ctx: &egui::Context, state: &mut AppState, input: &FrameInput, view: usize) {
    if !state.runtime.lab.fly_texture.active || state.runtime.lab.fly_texture.view != view {
        enter(state, view);
    }
    refresh_target(state);
    for (action, op) in OPS {
        if ctx.input_mut(|i| state.keys.take_action(&mut i.events, action)) {
            run(state, op);
        }
    }
    if input.pressed[SECONDARY] {
        let op = if input.modifiers.shift { FlyTextureOp::ApplyBrush } else { FlyTextureOp::Apply };
        run(state, op);
    } else if input.pressed[MIDDLE] {
        run(state, FlyTextureOp::Pick);
    }
}

pub fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(FlyTextureLayer)]
}

pub struct FlyTextureLayer;

const FILL: [f32; 4] = [1.0, 0.85, 0.2, 0.22];
const LINE: [f32; 4] = [1.0, 0.9, 0.3, 1.0];

impl OverlayLayer for FlyTextureLayer {
    fn name(&self) -> &'static str {
        "fly_texture"
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let fly = &state.runtime.lab.fly_texture;
        if !fly.active || view.index != fly.view || view.kind.is_2d() {
            return;
        }
        let Some(face) = fly.target else { return };
        let Some(poly) = state.doc.face_polygon(face) else { return };
        if poly.len() < 3 {
            return;
        }
        let normal = (poly[1] - poly[0]).cross(poly[2] - poly[1]).normalize_or_zero();
        let lift = normal * 0.15;
        let p = |v: DVec3| (v + lift).as_vec3().to_array();
        for i in 1..poly.len() - 1 {
            out.triangles.push(OverlayTri { p: [p(poly[0]), p(poly[i]), p(poly[i + 1])], color: FILL });
        }
        for i in 0..poly.len() {
            let a = p(poly[i]);
            let b = p(poly[(i + 1) % poly.len()]);
            out.lines_on_top.push(OverlayLine { a, b, color: LINE });
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let fly = &state.runtime.lab.fly_texture;
        if !fly.active || view.index != fly.view || view.kind.is_2d() {
            return;
        }
        let c = view.rect.center();
        let color = if fly.target.is_some() { Color32::WHITE } else { Color32::from_gray(200) };
        let stroke = Stroke::new(1.5, color);
        let shadow = Stroke::new(3.0, Color32::from_black_alpha(160));
        for s in [shadow, stroke] {
            painter.line_segment([c + vec2(-10.0, 0.0), c + vec2(-3.0, 0.0)], s);
            painter.line_segment([c + vec2(3.0, 0.0), c + vec2(10.0, 0.0)], s);
            painter.line_segment([c + vec2(0.0, -10.0), c + vec2(0.0, -3.0)], s);
            painter.line_segment([c + vec2(0.0, 3.0), c + vec2(0.0, 10.0)], s);
        }
        let target = match fly.target {
            Some(f) => tf!("viewport.fly_tex.target", f.solid, f.side, fly.material),
            None => t!("viewport.fly_tex.no_target").to_owned(),
        };
        paint::text_pill(painter, c + vec2(0.0, 28.0), Align2::CENTER_TOP, target, paint::TEXT);
        let key = |a: Action| state.keys.shortcut(a).map_or_else(|| "-".to_owned(), |s| shortcut_label(&s));
        let hint = tf!(
            "viewport.fly_tex.hint",
            key(Action::FlyApplyTexture),
            key(Action::FlyApplyTextureBrush),
            key(Action::FlyPickTexture),
            key(Action::FlyAlignTexture),
            state.current_material
        );
        paint::text_pill(
            painter,
            view.rect.center_bottom() + vec2(0.0, -12.0),
            Align2::CENTER_BOTTOM,
            hint,
            paint::TEXT_WEAK,
        );
    }
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    ("viewport.fly_tex.nothing", "No face under the crosshair.", "Aucune face sous le viseur."),
    ("viewport.fly_tex.picked", "Texture picked: {0}", "Texture prélevée : {0}"),
    ("viewport.fly_tex.failed", "Texturing failed: {0}", "Échec du texturage : {0}"),
    ("viewport.fly_tex.target", "Solid {0} side {1}: {2}", "Solide {0} face {1} : {2}"),
    ("viewport.fly_tex.no_target", "No face targeted", "Aucune face visée"),
    (
        "viewport.fly_tex.hint",
        "{0} apply  {1} whole brush  {2} pick  {3} align  |  Right click apply (Shift: brush)  Middle click pick  |  Texture: {4}",
        "{0} appliquer  {1} tout le brush  {2} prélever  {3} aligner  |  Clic droit appliquer (Maj : brush)  Clic milieu prélever  |  Texture : {4}",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn state_facing_wall() -> AppState {
        let mut state = hp_app::testing::test_state();
        state.cameras.cam3d.pos = DVec3::new(0.0, 0.0, 64.0);
        state.cameras.cam3d.yaw_deg = 90.0;
        state.cameras.cam3d.pitch_deg = 0.0;
        state
    }

    #[test]
    fn crosshair_texturing_applies_picks_and_undoes() {
        let mut state = state_facing_wall();
        let face = target_face(&state).expect("the wall is in front of the camera");
        state.current_material = "dev/b".into();
        enter(&mut state, 0);
        assert_eq!(state.runtime.lab.fly_texture.target, Some(face));
        assert!(run(&mut state, FlyTextureOp::Apply));
        assert_eq!(state.doc.face_side(face).unwrap().material, "dev/b");
        let other = state.doc.all_faces_of(&[ObjRef::Solid(face.solid)]).into_iter().find(|f| *f != face).unwrap();
        assert_eq!(state.doc.face_side(other).unwrap().material, "CONCRETE/CONCRETEFLOOR001A");
        state.current_material = "dev/c".into();
        assert!(run(&mut state, FlyTextureOp::ApplyBrush));
        assert_eq!(state.doc.face_side(other).unwrap().material, "dev/c");
        assert!(run(&mut state, FlyTextureOp::Align));
        state.current_material = "dev/z".into();
        assert!(run(&mut state, FlyTextureOp::Pick));
        assert_eq!(state.current_material, "dev/c");
        leave(&mut state);
        assert!(!state.runtime.lab.fly_texture.active);
        assert_eq!(state.runtime.lab.fly_texture.target, None);
    }

    #[test]
    fn nothing_targeted_does_nothing() {
        let mut state = hp_app::testing::test_state();
        state.cameras.cam3d.pos = DVec3::new(0.0, 0.0, 10000.0);
        assert!(!run(&mut state, FlyTextureOp::Apply));
    }

    #[test]
    fn actions_map_to_operations() {
        for (a, op) in OPS {
            assert_eq!(op_of(a), Some(op));
        }
        assert_eq!(op_of(Action::Undo), None);
    }
}
