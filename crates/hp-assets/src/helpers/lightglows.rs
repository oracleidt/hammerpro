use ahash::AHashSet;
use glam::DVec3;
use hp_editor::Id;
use hp_render::{LightGlowShape, MaterialId, ObjectId, WorldBeam};
use hp_vmf::Entity;

use crate::entities::entity_object;
use crate::material::{AnaLook, MatKind};
use crate::producer::{DerivedBuild, DerivedProducer, ProducerChanges, ProducerCx};

pub(crate) const GLOW_MATERIAL: &str = "sprites/light_glow02_add_noz";
const MAX_GLOWS: usize = 1024;
const SF_ONE_SIDED: i64 = 1;

pub(crate) fn is_light_glow(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case("env_lightglow")
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LightGlow {
    pub origin: DVec3,
    pub dir: DVec3,
    pub color: [f32; 3],
    pub shape: LightGlowShape,
    pub hdr_scale: f32,
    pub on: bool,
}

fn num(e: &Entity, key: &str) -> Option<f64> {
    e.get(key).and_then(|s| s.split_ascii_whitespace().next()?.parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn int(e: &Entity, key: &str) -> Option<f64> {
    num(e, key).map(f64::trunc)
}

pub(crate) fn light_glow(e: &Entity, on_override: Option<bool>) -> Option<LightGlow> {
    let origin = e.origin().filter(|o| o.is_finite())?;
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let dir = hp_geom::angles_to_vectors(angles).0.try_normalize().unwrap_or(DVec3::X);
    let flags = int(e, "spawnflags").unwrap_or(0.0) as i64;
    let rgb: Vec<f32> = e
        .get("rendercolor")
        .unwrap_or("255 255 255")
        .split_ascii_whitespace()
        .take(3)
        .filter_map(|c| c.parse::<f32>().ok())
        .filter(|c| c.is_finite())
        .map(|c| (c / 255.0).clamp(0.0, 1.0))
        .collect();
    let color = match rgb.as_slice() {
        [r, g, b] => [*r, *g, *b],
        _ => [1.0; 3],
    };
    let size = |key: &str| int(e, key).unwrap_or(30.0).clamp(0.0, 65_536.0) as f32;
    let dist = |key: &str, default: f64| int(e, key).unwrap_or(default).clamp(-1.0e6, 1.0e6) as f32;
    let disabled = int(e, "StartDisabled").is_some_and(|v| v != 0.0);
    Some(LightGlow {
        origin,
        dir,
        color,
        shape: LightGlowShape {
            size: [size("HorizontalGlowSize"), size("VerticalGlowSize")],
            min_dist: dist("MinDist", 500.0),
            max_dist: dist("MaxDist", 2000.0),
            outer_max_dist: dist("OuterMaxDist", 0.0),
            one_sided: flags & SF_ONE_SIDED != 0,
            proxy: num(e, "GlowProxySize").unwrap_or(2.0).clamp(0.0, 65_536.0) as f32,
        },
        hdr_scale: num(e, "HDRColorScale").unwrap_or(1.0).clamp(0.0, 64.0) as f32,
        on: on_override.unwrap_or(!disabled),
    })
}

pub(crate) fn glow_beam(g: &LightGlow, srgb: bool, material: MaterialId, object: ObjectId) -> Option<WorldBeam> {
    if !g.on || g.shape.size.contains(&0.0) || g.color == [0.0; 3] {
        return None;
    }
    Some(WorldBeam {
        object,
        material,
        start: g.origin.as_vec3().to_array(),
        end: (g.origin + g.dir).as_vec3().to_array(),
        width: 0.0,
        color: g.color,
        srgb_color: srgb,
        hdr_scale: g.hdr_scale,
        halo_scale: 0.0,
        glow: Some(g.shape),
    })
}

#[derive(Default)]
pub(crate) struct LightGlowProducer {
    ids: AHashSet<Id>,
    overrides: Vec<(Id, bool)>,
    waiting: bool,
    dirty: bool,
}

impl LightGlowProducer {
    fn overrides(cx: &ProducerCx<'_>) -> Vec<(Id, bool)> {
        let scene = cx.doc.scene();
        let mut out: Vec<(Id, bool)> = cx
            .assets
            .scene
            .projector_overrides
            .iter()
            .filter(|o| scene.entity(o.id).is_some_and(|n| is_light_glow(&n.entity.classname)))
            .filter_map(|o| Some((o.id, o.on?)))
            .collect();
        out.sort_unstable();
        out
    }
}

impl DerivedProducer for LightGlowProducer {
    fn name(&self) -> &'static str {
        "light glows"
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        if changes.full || changes.entities || changes.doc.everything {
            self.dirty = true;
            return true;
        }
        if self.waiting && !matches!(cx.assets.mats.ana_look(GLOW_MATERIAL), AnaLook::Loading) {
            self.dirty = true;
        }
        if Self::overrides(cx) != self.overrides {
            self.dirty = true;
        }
        let scene = cx.doc.scene();
        let touches =
            |id: &Id| self.ids.contains(id) || scene.entity(*id).is_some_and(|n| is_light_glow(&n.entity.classname));
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
        self.overrides = Self::overrides(cx);
        let mut found: Vec<(Id, LightGlow)> = cx
            .doc
            .scene()
            .entities()
            .iter()
            .filter(|(id, n)| is_light_glow(&n.entity.classname) && cx.doc.is_entity_visible(**id))
            .filter_map(|(id, n)| {
                let over = self.overrides.iter().find(|(o, _)| o == id).map(|(_, on)| *on);
                Some((*id, light_glow(&n.entity, over)?))
            })
            .collect();
        found.sort_unstable_by_key(|(id, _)| *id);
        found.truncate(MAX_GLOWS);
        let material = cx.assets.asset_material(GLOW_MATERIAL, MatKind::EntitySprite);
        let look = match cx.assets.mats.ana_look(GLOW_MATERIAL) {
            AnaLook::Ready(a) => Some(a.sprite_srgb),
            AnaLook::Missing => Some(true),
            AnaLook::Loading => None,
        };
        self.waiting = look.is_none();
        let srgb = look.unwrap_or(true);
        let mut beams = Vec::new();
        for (id, g) in found {
            self.ids.insert(id);
            beams.extend(glow_beam(&g, srgb, material, entity_object(id)));
        }
        DerivedBuild { beams: Some(beams), ..DerivedBuild::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(keys: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: "env_lightglow".to_owned(), ..Default::default() };
        e.set("origin", "0 0 128");
        for (k, v) in keys {
            e.set(k, *v);
        }
        e
    }

    #[test]
    fn keys_follow_the_game_defaults() {
        let g = light_glow(&ent(&[]), None).expect("glow");
        assert_eq!(g.shape.size, [30.0, 30.0]);
        assert_eq!((g.shape.min_dist, g.shape.max_dist, g.shape.outer_max_dist), (500.0, 2000.0, 0.0));
        assert_eq!((g.shape.proxy, g.hdr_scale), (2.0, 1.0));
        assert!(!g.shape.one_sided && g.on);
        assert_eq!(g.color, [1.0; 3]);
        assert!((g.dir - DVec3::X).length() < 1e-9);
        let g = light_glow(
            &ent(&[
                ("angles", "0 90 0"),
                ("spawnflags", "1"),
                ("rendercolor", "255 0 128"),
                ("HorizontalGlowSize", "64"),
                ("VerticalGlowSize", "16"),
                ("MinDist", "100"),
                ("MaxDist", "800"),
                ("OuterMaxDist", "3000"),
                ("GlowProxySize", "12.5"),
                ("HDRColorScale", "0.5"),
            ]),
            None,
        )
        .expect("glow");
        assert!(g.shape.one_sided, "Visible only from front");
        assert!((g.dir - DVec3::Y).length() < 1e-9, "the front follows the angles");
        assert_eq!(g.shape.size, [64.0, 16.0]);
        assert_eq!((g.shape.min_dist, g.shape.max_dist, g.shape.outer_max_dist), (100.0, 800.0, 3000.0));
        assert_eq!((g.shape.proxy, g.hdr_scale), (12.5, 0.5));
        assert!((g.color[2] - 128.0 / 255.0).abs() < 1e-6);
        assert!(!light_glow(&ent(&[("StartDisabled", "1")]), None).expect("glow").on);
        assert!(light_glow(&ent(&[("StartDisabled", "1")]), Some(true)).expect("glow").on, "Enable input");
    }

    #[test]
    fn the_glow_is_one_additive_sprite_on_the_entity() {
        let g = light_glow(&ent(&[("angles", "-90 0 0"), ("HDRColorScale", "2")]), None).expect("glow");
        let b = glow_beam(&g, false, 3, 9).expect("beam");
        assert_eq!((b.material, b.object, b.srgb_color, b.hdr_scale), (3, 9, false, 2.0));
        assert_eq!(b.start, [0.0, 0.0, 128.0]);
        assert!((glam::Vec3::from(b.end) - glam::Vec3::new(0.0, 0.0, 129.0)).length() < 1e-4, "pitch -90 faces up");
        assert_eq!(b.glow, Some(g.shape));
        let off = LightGlow { on: false, ..g.clone() };
        assert!(glow_beam(&off, true, 3, 9).is_none(), "StartDisabled");
        let flat = LightGlow { shape: LightGlowShape { size: [0.0, 30.0], ..g.shape }, ..g.clone() };
        assert!(glow_beam(&flat, true, 3, 9).is_none());
        let black = LightGlow { color: [0.0; 3], ..g };
        assert!(glow_beam(&black, true, 3, 9).is_none());
    }
}
