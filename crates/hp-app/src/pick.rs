use hp_editor::{Hit, ObjRef, PickOptions};
use hp_geom::Ray;
use hp_vmt::ToolKind;

use crate::AppState;
use crate::runtime::render::{HIDE_CLIPS, HIDE_HINTS, HIDE_NODRAW, HIDE_TRIGGERS, RenderDisplay};

pub fn guess_tool(name: &str) -> Option<ToolKind> {
    let lower = name.trim().replace('\\', "/").to_ascii_lowercase();
    if !lower.starts_with("tools/") {
        return None;
    }
    hp_vmt::Material::new("LightmappedGeneric").tool_kind(&lower)
}

pub fn tool_hide_group(kind: ToolKind) -> u8 {
    match kind {
        ToolKind::Nodraw => HIDE_NODRAW,
        ToolKind::Clip | ToolKind::PlayerClip | ToolKind::NpcClip => HIDE_CLIPS,
        ToolKind::Trigger => HIDE_TRIGGERS,
        ToolKind::Hint | ToolKind::Skip => HIDE_HINTS,
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HiddenFaces {
    pub all_tools: bool,
    pub groups: u8,
    pub sky_holes: bool,
}

impl HiddenFaces {
    pub fn of(state: &AppState) -> Self {
        let r = &state.settings.render;
        let display = RenderDisplay::from_settings(&state.settings, &state.runtime.render);
        HiddenFaces {
            all_tools: !state.scene_options().show_tool_textures,
            groups: display.hidden_tools,
            sky_holes: r.sky_see_through
                && (r.sky_preview || (r.skybox_3d && state.runtime.render.sky_camera.is_some())),
        }
    }

    pub fn none(&self) -> bool {
        !self.all_tools && self.groups == 0 && !self.sky_holes
    }

    pub fn hides(&self, tool: Option<ToolKind>) -> bool {
        let Some(kind) = tool else { return false };
        if self.all_tools || tool_hide_group(kind) & self.groups != 0 {
            return true;
        }
        self.sky_holes && matches!(kind, ToolKind::Skybox | ToolKind::Skybox2d)
    }
}

impl AppState {
    pub fn material_hidden_3d(&self, material: &str) -> bool {
        let hidden = HiddenFaces::of(self);
        !hidden.none() && hidden.hides(self.assets.material_tool(material))
    }

    fn solid_shown_3d(&self, obj: ObjRef) -> bool {
        let ObjRef::Solid(id) = obj else { return true };
        let r = &self.settings.render;
        let Some(n) = self.doc.scene().solid(id) else { return true };
        if n.has_disp() { r.show_displacements } else { !r.displacements_only }
    }

    fn with_view_options<R>(&self, opts: &PickOptions<'_>, f: impl FnOnce(&PickOptions<'_>) -> R) -> R {
        let hidden = HiddenFaces::of(self);
        let face_hidden = |m: &str| hidden.hides(self.assets.material_tool(m));
        let entity_ray = |e: &hp_vmf::Entity, ray: &Ray| self.assets.entity_ray(e, ray);
        let inner = opts.filter;
        let filter = |o: ObjRef| self.solid_shown_3d(o) && inner.is_none_or(|f| f(o));
        let view = PickOptions {
            filter: Some(&filter),
            face_hidden: if hidden.none() { opts.face_hidden } else { Some(opts.face_hidden.unwrap_or(&face_hidden)) },
            entity_ray: Some(opts.entity_ray.unwrap_or(&entity_ray)),
            ..*opts
        };
        f(&view)
    }

    pub fn view_pick(&self, ray: &Ray, opts: &PickOptions<'_>) -> Option<Hit> {
        self.with_view_options(opts, |o| self.doc.pick_ray(ray, o, &|e| self.assets.entity_bounds(e)))
    }

    pub fn view_pick_all(&self, ray: &Ray, opts: &PickOptions<'_>) -> Vec<Hit> {
        self.with_view_options(opts, |o| self.doc.pick_ray_all(ray, o, &|e| self.assets.entity_bounds(e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_faces_follow_the_view_settings() {
        let mut state = crate::testing::test_state();
        let h = HiddenFaces::of(&state);
        assert!(h.none());
        assert!(!state.material_hidden_3d("tools/toolsnodraw"));
        assert_eq!(guess_tool(r"TOOLS\ToolsNodraw"), Some(ToolKind::Nodraw));
        assert_eq!(guess_tool("brick/brickwall001a"), None);
        state.settings.render.hide_nodraw = true;
        assert!(state.material_hidden_3d("tools/toolsnodraw"));
        assert!(!state.material_hidden_3d("tools/toolsclip"));
        assert!(!state.material_hidden_3d("brick/brickwall001a"));
        state.settings.render.hide_nodraw = false;
        state.settings.render.hide_triggers = true;
        assert!(state.material_hidden_3d("tools/toolstrigger"));
        state.settings.render.hide_triggers = false;
        state.settings.render.hide_hints = true;
        assert!(state.material_hidden_3d("tools/toolshint") && state.material_hidden_3d("tools/toolsskip"));
        state.settings.render.hide_hints = false;
        assert!(!state.material_hidden_3d("tools/toolsskybox"));
        state.settings.render.sky_see_through = true;
        state.settings.render.sky_preview = true;
        assert!(state.material_hidden_3d("tools/toolsskybox"), "sky holes are not drawn");
        assert!(!state.material_hidden_3d("tools/toolsnodraw"));
        state.settings.render.sky_see_through = false;
        state.settings.view.show_tool_textures = false;
        for m in ["tools/toolsnodraw", "tools/toolsplayerclip", "tools/toolsskybox", "tools/toolsblack"] {
            assert!(state.material_hidden_3d(m), "{m}");
        }
        assert!(!state.material_hidden_3d("concrete/concretefloor001a"));
    }

    #[test]
    fn view_picks_skip_what_the_view_hides() {
        let mut state = crate::testing::test_state();
        let mut ids: Vec<ObjRef> = state.doc.scene().solids().keys().map(|id| ObjRef::Solid(*id)).collect();
        ids.sort_unstable_by_key(|o| match o {
            ObjRef::Solid(id) | ObjRef::Entity(id) => *id,
        });
        let Some(&first) = ids.first() else { panic!("the test map has brushes") };
        let b = state.doc.object_bounds(first, &|e| state.entity_bounds(e)).expect("bounds");
        let c = b.center();
        let ray = Ray::new(glam::DVec3::new(c.x, c.y, b.max.z + 1000.0), -glam::DVec3::Z);
        let raw = state.doc.pick_ray(&ray, &PickOptions::default(), &|e| state.entity_bounds(e)).map(|h| h.obj);
        assert_eq!(state.view_pick(&ray, &PickOptions::default()).map(|h| h.obj), raw);
        assert_eq!(state.view_pick_all(&ray, &PickOptions::default()).first().map(|h| h.obj), raw);
        state.settings.render.displacements_only = true;
        let only = state.view_pick_all(&ray, &PickOptions::default());
        assert!(
            only.iter().all(|h| match h.obj {
                ObjRef::Solid(id) => state.doc.scene().solid(id).is_some_and(|n| n.has_disp()),
                ObjRef::Entity(_) => true,
            }),
            "only displacements and entities"
        );
        let keep = |o: ObjRef| o != first;
        state.settings.render.displacements_only = false;
        let filtered = state.view_pick_all(&ray, &PickOptions { filter: Some(&keep), ..PickOptions::default() });
        assert!(filtered.iter().all(|h| h.obj != first), "the caller's filter still applies");
    }
}
