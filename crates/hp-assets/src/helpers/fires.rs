use ahash::AHashSet;
use hp_app::runtime::helpers::fire::{self, Fire, FireKind};
use hp_editor::Id;
use hp_render::{LightFalloff, LightKind, PreviewLight};

use crate::lights::SceneLight;
use crate::producer::{DerivedBuild, DerivedProducer, ProducerChanges, ProducerCx};

const MAX_FIRES: usize = 256;
const DLIGHT_MIN_LIGHT: f32 = 20.0 / 256.0;
const PLASMA_COLOR: [f32; 3] = [0.55, 0.7, 1.0];
const GLOW_HEIGHT: f64 = 0.4;
const GLOW_STRENGTH: f32 = 2.0;

pub(crate) fn glow_light(f: &Fire, on: bool) -> Option<PreviewLight> {
    if !on || !f.glow || f.size <= 0.0 {
        return None;
    }
    let radius = fire::glow_radius(f) as f32;
    let color = if f.kind == FireKind::Plasma { PLASMA_COLOR } else { fire::GLOW_COLOR };
    let pos = f.origin + glam::DVec3::Z * (f.size * GLOW_HEIGHT);
    Some(PreviewLight {
        kind: LightKind::Point,
        pos: pos.as_vec3().to_array(),
        color,
        intensity: GLOW_STRENGTH,
        range: radius,
        falloff: Some(LightFalloff {
            quadratic: 1.0 / (DLIGHT_MIN_LIGHT * radius * radius).max(1e-6),
            cap: 1.0e9,
            cutoff: radius,
            exponent: 1.0,
            dlight: true,
            ..LightFalloff::default()
        }),
    })
}

#[derive(Default)]
pub(crate) struct FireGlowProducer {
    ids: AHashSet<Id>,
    overrides: Vec<(Id, bool)>,
    shown: Option<bool>,
    dirty: bool,
}

impl FireGlowProducer {
    fn overrides(cx: &ProducerCx<'_>) -> Vec<(Id, bool)> {
        let scene = cx.doc.scene();
        let mut out: Vec<(Id, bool)> = cx
            .assets
            .scene
            .projector_overrides
            .iter()
            .filter(|o| scene.entity(o.id).is_some_and(|n| fire::is_fire(&n.entity.classname)))
            .filter_map(|o| Some((o.id, o.on?)))
            .collect();
        out.sort_unstable();
        out
    }
}

impl DerivedProducer for FireGlowProducer {
    fn name(&self) -> &'static str {
        "fire glows"
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        if changes.full || changes.entities || changes.doc.everything || self.shown != Some(cx.opts.show_helpers) {
            self.dirty = true;
            return true;
        }
        if Self::overrides(cx) != self.overrides {
            self.dirty = true;
        }
        let scene = cx.doc.scene();
        let touches =
            |id: &Id| self.ids.contains(id) || scene.entity(*id).is_some_and(|n| fire::is_fire(&n.entity.classname));
        if changes.doc.entities_changed.iter().any(touches)
            || changes.doc.entities_removed.iter().any(|id| self.ids.contains(id))
            || changes.doc.visibility_changed
        {
            self.dirty = true;
        }
        self.dirty
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        self.dirty = false;
        self.ids.clear();
        self.shown = Some(cx.opts.show_helpers);
        self.overrides = Self::overrides(cx);
        let mut lights = Vec::new();
        if cx.opts.show_helpers {
            let mut found: Vec<(Id, Fire)> = cx
                .doc
                .scene()
                .entities()
                .iter()
                .filter(|(id, n)| fire::is_fire(&n.entity.classname) && cx.doc.is_entity_visible(**id))
                .filter_map(|(id, n)| Some((*id, fire::fire(&n.entity)?)))
                .collect();
            found.sort_unstable_by_key(|(id, _)| *id);
            found.truncate(MAX_FIRES);
            for (id, f) in found {
                self.ids.insert(id);
                let on = self.overrides.iter().find(|(o, _)| *o == id).is_none_or(|(_, on)| *on);
                lights.extend(glow_light(&f, on).map(SceneLight::same));
            }
        }
        DerivedBuild { lights: Some(lights), ..DerivedBuild::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fire_of(keys: &[(&str, &str)]) -> Fire {
        let mut e = hp_vmf::Entity { classname: "env_fire".to_owned(), ..Default::default() };
        e.set("origin", "0 0 0");
        for (k, v) in keys {
            e.set(k, *v);
        }
        fire::fire(&e).expect("fire")
    }

    #[test]
    fn the_orange_glow_follows_no_glow_and_the_fire_state() {
        let f = fire_of(&[("firesize", "64")]);
        let l = glow_light(&f, true).expect("glow");
        assert_eq!(l.color, fire::GLOW_COLOR);
        assert!((l.range - 256.0).abs() < 1e-3 && l.pos[2] > 0.0);
        assert!(l.falloff.is_some_and(|f| f.dlight), "an engine dynamic light");
        assert!(glow_light(&f, false).is_none(), "Extinguish");
        assert!(glow_light(&fire_of(&[("spawnflags", "32")]), true).is_none(), "No glow");
        assert_eq!(glow_light(&fire_of(&[("firetype", "1")]), true).expect("plasma").color, PLASMA_COLOR);
    }
}
