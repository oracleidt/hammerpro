pub mod fish;
pub mod geometry;
pub mod names;

use std::cell::{Cell, Ref, RefCell};

use hp_editor::Document;

use crate::settings::Settings;

pub use names::NameIndex;

pub fn path_clone_link(state: &crate::AppState, objs: &[hp_editor::ObjRef]) -> Option<(hp_editor::Id, &'static str)> {
    let [hp_editor::ObjRef::Entity(id)] = objs else { return None };
    let scene = state.doc.scene();
    if scene.is_brush_entity(*id) {
        return None;
    }
    let class = &scene.entity(*id)?.entity.classname;
    let from_fgd = state.assets.fgd().and_then(|f| f.class(class)).and_then(|c| match c.kind {
        hp_fgd::ClassKind::Move => Some("target"),
        hp_fgd::ClassKind::KeyFrame => Some("NextKey"),
        _ => None,
    });
    from_fgd.or_else(|| Document::known_path_next_key(class)).map(|key| (*id, key))
}

pub fn fgd_target_key(fgd: &hp_fgd::ResolvedFgd, class: &str, key: &str) -> Option<bool> {
    let p = fgd.class(class)?.property(key)?;
    Some(matches!(
        p.kind,
        hp_fgd::PropertyType::TargetDestination
            | hp_fgd::PropertyType::TargetNameOrClass
            | hp_fgd::PropertyType::FilterClass
    ))
}

pub fn array_frame(state: &crate::AppState) -> hp_editor::ArrayFrame {
    let frame = state.target_grid().frame;
    let step = if state.snap_enabled() { frame.step.max(glam::DVec3::ZERO) } else { glam::DVec3::ZERO };
    hp_editor::ArrayFrame { origin: frame.origin, axes: frame.axes, step }
}

pub fn array_duplicate(
    state: &mut crate::AppState,
    objs: &[hp_editor::ObjRef],
    spec: &hp_editor::ArraySpec,
) -> hp_editor::EditResult<(hp_editor::ArrayPlan, Vec<hp_editor::ObjRef>)> {
    let fgd = state.assets.fgd().filter(|f| !f.is_empty());
    let targets = |class: &str, key: &str| fgd.and_then(|f| fgd_target_key(f, class, key));
    let assets = &state.assets;
    state.doc.array_copies(objs, spec, &|e| assets.entity_bounds(e), Some(&targets))
}

pub fn duplicate_linking_paths(
    state: &mut crate::AppState,
    objs: &[hp_editor::ObjRef],
    m: glam::DMat4,
    anchor: glam::DVec3,
) -> hp_editor::EditResult<Vec<hp_editor::ObjRef>> {
    let keep = state.settings.selection.clone_keeps_visgroups;
    let link = path_clone_link(state, objs);
    let fgd = state.assets.fgd().filter(|f| !f.is_empty());
    let targets = |class: &str, key: &str| fgd.and_then(|f| fgd_target_key(f, class, key));
    state.doc.transaction("Duplicate", |doc| {
        let new = doc.duplicate_transformed_with(objs, m, anchor, keep, Some(&targets))?;
        if let (Some((src, key)), [hp_editor::ObjRef::Entity(copy)]) = (link, new.as_slice()) {
            doc.link_cloned_path_node(src, *copy, key)?;
        }
        Ok(new)
    })
}

pub fn brush_entity_needs_origin(fgd: Option<&hp_fgd::ResolvedFgd>, class: &str) -> bool {
    !hp_vmf::is_world_particle_volume(class)
        && fgd.and_then(|f| f.class(class.trim())).is_some_and(|c| c.is_solid() && c.property("origin").is_some())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneDisplay {
    pub overlays: bool,
    pub ropes: bool,
    pub simulate_ropes: bool,
    pub editor_objects: bool,
    pub game_look: bool,
}

impl Default for SceneDisplay {
    fn default() -> Self {
        Self { overlays: true, ropes: true, simulate_ropes: false, editor_objects: true, game_look: false }
    }
}

impl SceneDisplay {
    pub fn from_settings(s: &Settings) -> Self {
        let h = &s.helpers;
        Self {
            overlays: h.show_overlays,
            ropes: h.show_ropes,
            simulate_ropes: h.simulate_ropes,
            editor_objects: h.show_editor_objects,
            game_look: false,
        }
    }
}

thread_local! {
    static DISPLAY: Cell<SceneDisplay> = Cell::new(SceneDisplay::default());
}

pub fn publish_scene_display(display: SceneDisplay) {
    DISPLAY.with(|d| d.set(display));
}

pub fn scene_display() -> SceneDisplay {
    DISPLAY.with(Cell::get)
}

#[derive(Debug, Default)]
pub struct HelpersRuntime {
    names: RefCell<NameIndex>,
    point_entities_hidden: Cell<Option<(u64, bool)>>,
}

impl HelpersRuntime {
    pub fn names(&self, doc: &Document) -> Ref<'_, NameIndex> {
        if let Ok(mut n) = self.names.try_borrow_mut() {
            n.refresh(doc);
        }
        self.names.borrow()
    }

    pub fn point_entities_hidden(&self, doc: &Document) -> bool {
        let rev = doc.revision();
        if let Some((r, v)) = self.point_entities_hidden.get()
            && r == rev
        {
            return v;
        }
        let scene = doc.scene();
        let mut any = false;
        let mut all_hidden = true;
        for (id, n) in scene.entities() {
            if scene.is_brush_entity(*id) {
                continue;
            }
            any = true;
            if n.entity.editor.visgroup_auto_shown {
                all_hidden = false;
                break;
            }
        }
        let v = any && all_hidden;
        self.point_entities_hidden.set(Some((rev, v)));
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_follows_settings_per_thread() {
        let mut s = Settings::default();
        assert_eq!(SceneDisplay::from_settings(&s), SceneDisplay::default());
        s.helpers.show_overlays = false;
        s.helpers.simulate_ropes = true;
        let d = SceneDisplay::from_settings(&s);
        publish_scene_display(d);
        assert_eq!(scene_display(), d);
        let other = std::thread::spawn(scene_display).join().expect("thread");
        assert_eq!(other, SceneDisplay::default(), "each thread has its own");
        publish_scene_display(SceneDisplay::default());
    }

    #[test]
    fn point_entities_hidden_state() {
        let mut state = crate::testing::test_state();
        let rt = HelpersRuntime::default();
        assert!(!rt.point_entities_hidden(&state.doc));
        let point = |scene: &hp_editor::Scene, obj: hp_editor::ObjRef| match obj {
            hp_editor::ObjRef::Entity(id) => !scene.is_brush_entity(id),
            hp_editor::ObjRef::Solid(_) => false,
        };
        assert_eq!(state.doc.set_auto_visibility(&point, false), 2);
        assert!(rt.point_entities_hidden(&state.doc));
        assert_eq!(rt.names(&state.doc).named("main_light"), &[5]);
        let a = rt.names(&state.doc);
        let b = rt.names(&state.doc);
        assert_eq!(a.fingerprint(), b.fingerprint());
    }
}
