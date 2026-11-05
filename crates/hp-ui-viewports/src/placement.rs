use glam::{DVec2, DVec3};
use hp_app::AppState;
use hp_app::grid::GridFrame;
use hp_app::state::{ModelInfo, ModelState};
use hp_app::view::ViewInfo;
use hp_editor::{FaceRef, ObjRef, PickOptions};
use hp_geom::Ray;

pub const DEFAULT_HALF: f64 = 8.0;
pub const MISS_DISTANCE: f64 = 512.0;
const FREE_REST_STEP: f64 = 0.01;
const ORIGIN_STEP: f64 = 0.001;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub point: DVec3,
    pub normal: DVec3,
    pub face: Option<FaceRef>,
    pub on_surface: bool,
    pub is_2d: bool,
}

pub fn rest_on_surface(point: DVec3, normal: DVec3, min: DVec3, max: DVec3) -> DVec3 {
    let n = normal.normalize_or_zero();
    if n == DVec3::ZERO || !min.is_finite() || !max.is_finite() {
        return point;
    }
    let lowest = (n.x * min.x).min(n.x * max.x) + (n.y * min.y).min(n.y * max.y) + (n.z * min.z).min(n.z * max.z);
    point - n * lowest
}

fn clean(x: f64) -> f64 {
    if !x.is_finite() {
        return 0.0;
    }
    let r = x.round();
    if (x - r).abs() < 1e-6 { r + 0.0 } else { x + 0.0 }
}

pub fn round_to(x: f64, step: f64) -> f64 {
    if !(step.is_finite() && step > 0.0 && x.is_finite()) {
        return x;
    }
    let r = (x / step).round() * step;
    let digits = (0..12).find(|d| {
        let scaled = step * 10f64.powi(*d);
        (scaled - scaled.round()).abs() < 1e-9 * scaled.max(1.0)
    });
    let scale = 10f64.powi(digits.unwrap_or(12));
    clean((r * scale).round() / scale)
}

fn dominant_axis(n: DVec3) -> usize {
    let a = n.abs();
    if a.z >= a.x && a.z >= a.y {
        2
    } else if a.x >= a.y {
        0
    } else {
        1
    }
}

pub fn snap_on_plane(p: DVec3, n: DVec3, grid: f64) -> DVec3 {
    if !(grid.is_finite() && grid > 0.0) {
        return p;
    }
    let Some(n) = n.try_normalize() else { return p };
    let k = dominant_axis(n);
    let d = n.dot(p);
    let mut q = p;
    let mut rest = 0.0;
    for i in 0..3 {
        if i != k {
            q[i] = if p[i].is_finite() { clean((p[i] / grid).round() * grid) } else { p[i] };
            rest += n[i] * q[i];
        }
    }
    if n[k].abs() < 1e-9 {
        return p;
    }
    q[k] = clean((d - rest) / n[k]);
    if q.is_finite() { q } else { p }
}

pub fn rest_origin(point: DVec3, normal: DVec3, min: DVec3, max: DVec3, grid: f64) -> DVec3 {
    let frame = (grid.is_finite() && grid > 0.0).then(|| GridFrame::uniform(grid));
    rest_origin_on(point, normal, min, max, frame.as_ref())
}

pub fn rest_origin_on(point: DVec3, normal: DVec3, min: DVec3, max: DVec3, grid: Option<&GridFrame>) -> DVec3 {
    let n = normal.try_normalize().unwrap_or(DVec3::Z);
    if !min.is_finite() || !max.is_finite() || !point.is_finite() {
        return point;
    }
    let p = match grid {
        Some(f) => {
            let q = f.on_plane(point, n);
            DVec3::new(clean(q.x), clean(q.y), clean(q.z))
        }
        None => point,
    };
    let push = (-n.x * min.x).max(-n.x * max.x) + (-n.y * min.y).max(-n.y * max.y) + (-n.z * min.z).max(-n.z * max.z);
    let step = grid.map_or(FREE_REST_STEP, |f| f.nominal().min(1.0));
    let o = p + n * round_to(push, step);
    DVec3::new(round_to(o.x, ORIGIN_STEP), round_to(o.y, ORIGIN_STEP), round_to(o.z, ORIGIN_STEP))
}

pub fn box_rest_offset(min: DVec3, max: DVec3, point: DVec3, normal: DVec3) -> DVec3 {
    let center = (min + max) * 0.5;
    let half = (max - min) * 0.5;
    rest_on_surface(point, normal, -half, half) - center
}

pub fn box_offset_2d(min: DVec3, max: DVec3, point: DVec3, axes: (usize, usize)) -> DVec3 {
    let center = (min + max) * 0.5;
    let mut off = DVec3::ZERO;
    off[axes.0] = point[axes.0] - center[axes.0];
    off[axes.1] = point[axes.1] - center[axes.1];
    off
}

pub fn aligned_axis(normal: DVec3) -> Option<usize> {
    let n = normal.normalize_or_zero();
    (0..3).find(|&i| n[i].abs() > 0.999)
}

pub fn miss_point(origin: DVec3, dir: DVec3) -> DVec3 {
    let d = dir.normalize_or(DVec3::X);
    if d.z < -1e-3 {
        let t = -origin.z / d.z;
        if t > 0.0 && t < 16384.0 {
            return origin + d * t;
        }
    }
    origin + d * MISS_DISTANCE
}

pub fn view_ray(view: &ViewInfo, pos: egui::Pos2) -> Ray {
    let (o, d) = view.ray(pos);
    Ray::new(o, d)
}

pub fn pick_surface(state: &AppState, view: &ViewInfo, pos: egui::Pos2) -> Option<hp_editor::Hit> {
    let ray = view_ray(view, pos);
    if !ray.origin.is_finite() || !ray.dir.is_finite() {
        return None;
    }
    state.view_pick(&ray, &PickOptions::default())
}

pub fn point_2d(state: &AppState, view: &ViewInfo, pos: egui::Pos2, depth: f64) -> DVec3 {
    let p = view.screen_to_2d(pos);
    let p = if state.snap_enabled() { state.grid_frame(view.kind).snap_2d(view.kind, p) } else { p };
    let p = p.clamp(DVec2::splat(-hp_geom::MAX_COORD), DVec2::splat(hp_geom::MAX_COORD));
    view.kind.from_2d(p, depth)
}

pub fn target_at(state: &AppState, view: &ViewInfo, pos: egui::Pos2, depth_2d: f64) -> Target {
    if view.is_2d() {
        return Target {
            point: point_2d(state, view, pos, depth_2d),
            normal: view.kind.toward_viewer(),
            face: None,
            on_surface: false,
            is_2d: true,
        };
    }
    match pick_surface(state, view, pos) {
        Some(hit) => {
            let face = match (hit.obj, hit.face) {
                (ObjRef::Solid(id), Some(side)) => Some(FaceRef::new(id, side)),
                _ => None,
            };
            let normal = if hit.normal.is_finite() && hit.normal != DVec3::ZERO { hit.normal } else { DVec3::Z };
            Target { point: hit.point, normal, face, on_surface: true, is_2d: false }
        }
        None => {
            let ray = view_ray(view, pos);
            Target {
                point: miss_point(ray.origin, ray.dir),
                normal: DVec3::Z,
                face: None,
                on_surface: false,
                is_2d: false,
            }
        }
    }
}

pub fn model_box(info: &ModelInfo) -> Option<(DVec3, DVec3)> {
    let usable =
        |(a, b): &(DVec3, DVec3)| a.is_finite() && b.is_finite() && a.cmple(*b).all() && (*b - *a).max_element() > 0.0;
    let hull = info.hull_min.zip(info.hull_max).filter(usable);
    hull.or_else(|| Some((info.bounds_min, info.bounds_max)).filter(usable))
}

pub fn is_prop_class(state: &AppState, class: &str) -> bool {
    let def = state.assets.fgd().and_then(|f| f.class(class));
    if def.is_some_and(|c| {
        c.studio().is_some_and(|s| s.model.is_some())
            || c.property("model").and_then(|p| p.default_value()).is_some_and(|m| !m.trim().is_empty())
    }) {
        return false;
    }
    class.trim().to_ascii_lowercase().starts_with("prop_")
        || state
            .assets
            .fgd()
            .and_then(|f| f.class(class))
            .and_then(|c| c.studio())
            .is_some_and(|s| s.is_prop && s.model.is_none())
}

pub fn placement_bounds(state: &mut AppState, class: &str, model: Option<&str>) -> (DVec3, DVec3) {
    let fallback = (DVec3::splat(-DEFAULT_HALF), DVec3::splat(DEFAULT_HALF));
    let def = state.assets.fgd().and_then(|f| f.class(class));
    let size = def
        .and_then(|c| c.size())
        .map(|(a, b)| (DVec3::from_array(a), DVec3::from_array(b)))
        .filter(|(a, b)| a.is_finite() && b.is_finite());
    let fixed_model = def.and_then(|c| c.studio()).and_then(|s| s.model).or_else(|| {
        def.and_then(|c| c.property("model")).and_then(|p| p.default_value()).filter(|m| !m.trim().is_empty())
    });
    let model = if is_prop_class(state, class) {
        model.map(str::to_owned).or_else(|| state.current_model.clone())
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
    size.unwrap_or(fallback)
}

pub fn model_keys(path: &str, skin: u32) -> Vec<(&'static str, String)> {
    let mut kv = vec![("model", path.to_owned())];
    if skin > 0 {
        kv.push(("skin", skin.to_string()));
    }
    kv
}

pub fn is_brush_class(state: &AppState, class: &str) -> bool {
    state.assets.fgd().and_then(|f| f.class(class)).is_some_and(|c| c.is_solid())
}

pub fn entity_keyvalues(state: &AppState, class: &str, extra: &[(&str, &str)]) -> Vec<(String, String)> {
    let mut kv = state.assets.fgd().and_then(|f| f.class(class)).map(|c| c.default_keyvalues()).unwrap_or_default();
    kv.retain(|(k, _)| {
        !k.eq_ignore_ascii_case("origin")
            && !k.eq_ignore_ascii_case("classname")
            && !k.eq_ignore_ascii_case("id")
            && !extra.iter().any(|(e, _)| e.eq_ignore_ascii_case(k))
    });
    kv.extend(extra.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())));
    kv
}

pub fn origin_for(state: &AppState, target: &Target, local_min: DVec3, local_max: DVec3) -> DVec3 {
    if target.is_2d || !target.on_surface {
        return target.point;
    }
    let surface = target.face.map(|f| hp_editor::ObjRef::Solid(f.solid));
    let grid = state.snap_enabled().then(|| state.surface_grid_frame(hp_app::ViewKind::Perspective, surface));
    rest_origin_on(target.point, target.normal, local_min, local_max, grid.as_ref())
}

pub fn create_entity(
    state: &mut AppState,
    class: &str,
    target: &Target,
    local_bounds: (DVec3, DVec3),
    extra: &[(&str, &str)],
) -> hp_editor::EditResult<hp_editor::Id> {
    let origin = origin_for(state, target, local_bounds.0, local_bounds.1);
    let kv = entity_keyvalues(state, class, extra);
    state.doc.create_point_entity(class, origin, &kv)
}

pub fn prop_for_model(state: &mut AppState, path: &str) -> (&'static str, Option<(DVec3, DVec3)>, Option<String>) {
    match state.assets.model_info(path) {
        ModelState::Ready(info) => {
            let class = if info.is_static_prop { "prop_static" } else { "prop_dynamic" };
            (class, model_box(&info), None)
        }
        ModelState::Loading => ("prop_static", None, None),
        ModelState::Failed(e) => ("prop_static", None, Some(e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resting_on_surfaces() {
        let min = DVec3::new(-16.0, -16.0, 0.0);
        let max = DVec3::new(16.0, 16.0, 48.0);
        assert_eq!(rest_on_surface(DVec3::new(5.0, 6.0, 64.0), DVec3::Z, min, max), DVec3::new(5.0, 6.0, 64.0));
        let c = rest_on_surface(DVec3::ZERO, DVec3::Z, DVec3::splat(-8.0), DVec3::splat(8.0));
        assert_eq!(c, DVec3::new(0.0, 0.0, 8.0));
        let c = rest_on_surface(DVec3::new(0.0, 0.0, 128.0), -DVec3::Z, min, max);
        assert_eq!(c, DVec3::new(0.0, 0.0, 80.0));
        let c = rest_on_surface(DVec3::new(100.0, 0.0, 32.0), DVec3::X, min, max);
        assert_eq!(c, DVec3::new(116.0, 0.0, 32.0));
        assert_eq!(rest_on_surface(DVec3::ONE, DVec3::ZERO, min, max), DVec3::ONE);
    }

    #[test]
    fn box_offsets() {
        let off = box_rest_offset(
            DVec3::new(100.0, 100.0, 10.0),
            DVec3::new(132.0, 164.0, 74.0),
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::Z,
        );
        let min = DVec3::new(100.0, 100.0, 10.0) + off;
        let max = DVec3::new(132.0, 164.0, 74.0) + off;
        assert_eq!((min + max) * 0.5 * DVec3::new(1.0, 1.0, 0.0), DVec3::ZERO);
        assert!(min.z.abs() < 1e-9);
        let off = box_offset_2d(
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(64.0, 64.0, 64.0),
            DVec3::new(100.0, 0.0, 500.0),
            (0, 2),
        );
        assert_eq!(off, DVec3::new(68.0, 0.0, 468.0));
    }

    #[test]
    fn snapping_on_surfaces() {
        assert_eq!(snap_on_plane(DVec3::new(7.0, 9.0, 3.3), DVec3::Z, 16.0), DVec3::new(0.0, 16.0, 3.3));
        assert_eq!(snap_on_plane(DVec3::new(7.0, 9.0, 3.3), -DVec3::X, 16.0), DVec3::new(7.0, 16.0, 0.0));
        let slanted = DVec3::new(1.0, 0.0, 1.0);
        let q = snap_on_plane(DVec3::new(7.0, 9.0, 3.3), slanted, 16.0);
        assert_eq!((q.x, q.y), (0.0, 16.0));
        assert!((q.z - 10.3).abs() < 1e-9, "{q:?}");
        assert_eq!(snap_on_plane(DVec3::new(7.0, 9.0, 3.3), DVec3::Z, 0.0), DVec3::new(7.0, 9.0, 3.3));
        assert_eq!(aligned_axis(DVec3::new(0.0, -2.0, 0.0)), Some(1));
        assert_eq!(aligned_axis(DVec3::ZERO), None);
    }

    #[test]
    fn resting_origins_follow_the_entity_tool() {
        let (lo, hi) = (DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 72.0));
        let floor = rest_origin(DVec3::new(-170.3, -390.2, 0.0), DVec3::Z, lo, hi, 16.0);
        assert_eq!(floor, DVec3::new(-176.0, -384.0, 0.0));
        let wall = rest_origin(DVec3::new(-64.0, 3.0, 61.7), -DVec3::X, lo, hi, 16.0);
        assert_eq!(wall, DVec3::new(-80.0, 0.0, 64.0));
        let mesh = (DVec3::new(-6.8, -21.0, 0.113_388_687), DVec3::new(6.8, 5.25, 71.27));
        let o = rest_origin(DVec3::new(0.0, 0.0, 0.0), DVec3::Z, mesh.0, mesh.1, 16.0);
        assert_eq!(o, DVec3::ZERO);
        let o = rest_origin(DVec3::new(1.234_567_89, 2.0, 0.0), DVec3::Z, mesh.0, mesh.1, 0.0);
        assert_eq!(o, DVec3::new(1.235, 2.0, -0.11));
        let n = DVec3::new(0.0, 1.0, 1.0).normalize();
        let o = rest_origin(DVec3::new(5.0, 3.0, -3.0), n, DVec3::splat(-8.0), DVec3::splat(8.0), 16.0);
        assert!(o.x == 0.0 && o.is_finite(), "{o:?}");
        let on_plane = o - n * round_to(16.0 * std::f64::consts::FRAC_1_SQRT_2, 1.0);
        assert!(n.dot(on_plane).abs() < 0.01, "{o:?}");
        assert_eq!(rest_origin(DVec3::ONE, DVec3::Z, DVec3::splat(f64::NAN), DVec3::ONE, 16.0), DVec3::ONE);
        assert!(rest_origin(DVec3::ONE, DVec3::ZERO, lo, hi, 16.0).is_finite());
        assert_eq!(round_to(0.07, 0.01), 0.07);
        assert_eq!(round_to(-0.000_1, 1.0), 0.0);
        assert!(round_to(-0.000_1, 1.0).is_sign_positive());
    }

    #[test]
    fn model_boxes_prefer_the_hull() {
        let mut info = ModelInfo {
            bounds_min: DVec3::new(-6.8, -21.0, 0.113),
            bounds_max: DVec3::new(6.8, 5.25, 71.27),
            ..Default::default()
        };
        assert_eq!(model_box(&info), Some((info.bounds_min, info.bounds_max)));
        info.hull_min = Some(DVec3::new(-16.0, -16.0, 0.0));
        info.hull_max = Some(DVec3::new(16.0, 16.0, 72.0));
        assert_eq!(model_box(&info), Some((DVec3::new(-16.0, -16.0, 0.0), DVec3::new(16.0, 16.0, 72.0))));
        info.hull_max = Some(DVec3::new(-16.0, -16.0, 0.0));
        assert_eq!(model_box(&info).map(|b| b.0), Some(info.bounds_min), "empty hull ignored");
        info.bounds_min = DVec3::splat(f64::NAN);
        assert_eq!(model_box(&info), None);
        assert_eq!(model_keys("models/a.mdl", 0), [("model", "models/a.mdl".to_owned())]);
        assert_eq!(model_keys("models/a.mdl", 2)[1], ("skin", "2".to_owned()));
    }

    #[test]
    fn misses() {
        let p = miss_point(DVec3::new(0.0, 0.0, 100.0), DVec3::new(1.0, 0.0, -1.0));
        assert!((p - DVec3::new(100.0, 0.0, 0.0)).length() < 1e-9);
        let p = miss_point(DVec3::ZERO, DVec3::X);
        assert_eq!(p, DVec3::new(MISS_DISTANCE, 0.0, 0.0));
    }

    #[test]
    fn keyvalues_without_fgd() {
        let state = hp_app::testing::test_state();
        let kv = entity_keyvalues(&state, "prop_static", &[("model", "models/a.mdl")]);
        assert_eq!(kv, vec![("model".to_owned(), "models/a.mdl".to_owned())]);
        let mut state = state;
        assert_eq!(placement_bounds(&mut state, "anything", None), (DVec3::splat(-8.0), DVec3::splat(8.0)));
        assert!(is_prop_class(&state, "Prop_Physics"));
        assert!(!is_prop_class(&state, "info_target"));
        assert!(!is_brush_class(&state, "func_detail"));
    }

    #[test]
    fn creating_entities() {
        let mut state = hp_app::testing::test_state();
        let before = state.doc.scene().entities().len();
        let target =
            Target { point: DVec3::new(3.0, 5.0, 0.0), normal: DVec3::Z, face: None, on_surface: true, is_2d: false };
        let id = create_entity(&mut state, "info_target", &target, (DVec3::splat(-8.0), DVec3::splat(8.0)), &[]);
        let Ok(id) = id else { panic!("entity creation failed: {id:?}") };
        assert_eq!(state.doc.scene().entities().len(), before + 1);
        let Some(node) = state.doc.scene().entity(id) else { panic!("entity missing") };
        assert_eq!(node.entity.origin(), Some(DVec3::new(0.0, 0.0, 8.0)));
        assert_eq!(node.entity.get("origin"), Some("0 0 8"));
        assert!(state.doc.selection().contains(ObjRef::Entity(id)));
        let (class, bounds, err) = prop_for_model(&mut state, "models/props_c17/oildrum001.mdl");
        assert_eq!(class, "prop_static");
        assert!(bounds.is_some() && err.is_none());
    }
}
