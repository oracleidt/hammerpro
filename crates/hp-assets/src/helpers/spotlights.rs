use ahash::AHashSet;
use glam::DVec3;
use hp_editor::{Document, Id, Owner, SolidNode};
use hp_render::{LightFalloff, LightKind, MaterialId, ObjectId, PreviewLight, WorldBeam};
use hp_vmf::Entity;

use crate::entities::entity_object;
use crate::lights::SceneLight;
use crate::material::{AnaLook, MatKind};
use crate::producer::{DerivedBuild, DerivedProducer, ProducerChanges, ProducerCx};

pub(crate) const HALO_MATERIAL: &str = "sprites/light_glow03";
pub(crate) const BEAM_MATERIAL: &str = "sprites/glow_test02";
const DEFAULT_LENGTH: f64 = 500.0;
const DEFAULT_WIDTH: f64 = 50.0;
const MAX_BEAM_WIDTH: f64 = 102.3;
const HALO_SCALE: f32 = 60.0;
const BEAM_BRIGHTNESS: f32 = 64.0 / 255.0;
const LIGHT_SCALE: f64 = 1.8;
const LIGHT_RADIUS: f64 = 3.0;
const DLIGHT_MIN_LIGHT: f32 = 20.0 / 256.0;
const MAX_SPOTLIGHTS: usize = 256;
const SF_START_ON: i64 = 1;
const SF_NO_DYNAMIC_LIGHT: i64 = 2;

pub(crate) fn is_spotlight(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case("point_spotlight")
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Spotlight {
    pub origin: DVec3,
    pub dir: DVec3,
    pub length: f64,
    pub width: f64,
    pub color: [f32; 3],
    pub hdr_scale: f32,
    pub on: bool,
    pub dynamic_light: bool,
}

fn num(e: &Entity, key: &str) -> Option<f64> {
    e.get(key).and_then(|s| s.split_ascii_whitespace().next()?.parse::<f64>().ok()).filter(|v| v.is_finite())
}

pub(crate) fn spotlight(e: &Entity, on_override: Option<bool>) -> Option<Spotlight> {
    let origin = e.origin().filter(|o| o.is_finite())?;
    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let dir = hp_geom::angles_to_vectors(angles).0.try_normalize()?;
    let flags = num(e, "spawnflags").map_or(SF_START_ON, |f| f as i64);
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
    Some(Spotlight {
        origin,
        dir,
        length: num(e, "SpotlightLength").unwrap_or(DEFAULT_LENGTH).clamp(1.0, 65_536.0),
        width: num(e, "SpotlightWidth").unwrap_or(DEFAULT_WIDTH).clamp(1.0, MAX_BEAM_WIDTH),
        color,
        hdr_scale: num(e, "HDRColorScale").unwrap_or(1.0).clamp(0.0, 64.0) as f32,
        on: on_override.unwrap_or(flags & SF_START_ON != 0),
        dynamic_light: flags & SF_NO_DYNAMIC_LIGHT == 0,
    })
}

const PASSABLE_TOOLS: [&str; 9] = [
    "tools/toolstrigger",
    "tools/toolsclip",
    "tools/toolsplayerclip",
    "tools/toolsnpcclip",
    "tools/toolshint",
    "tools/toolsskip",
    "tools/toolsareaportal",
    "tools/toolsoccluder",
    "tools/toolsfog",
];

fn blocks_beam(doc: &Document, node: &SolidNode) -> bool {
    let owner_ok = match node.owner {
        Owner::World => true,
        Owner::Entity(e) => doc.scene().entity(e).is_some_and(|n| {
            let c = n.entity.classname.trim().to_ascii_lowercase();
            !c.starts_with("trigger_") && !c.starts_with("func_areaportal") && c != "func_clip_vphysics"
        }),
    };
    owner_ok
        && !node.solid.sides.iter().all(|s| {
            let m = s.material.replace('\\', "/").to_ascii_lowercase();
            PASSABLE_TOOLS.iter().any(|t| m == *t)
        })
}

pub(crate) fn beam_end(doc: &Document, s: &Spotlight) -> DVec3 {
    let end = s.origin + s.dir * s.length;
    let filter = |n: &SolidNode| blocks_beam(doc, n);
    match doc.trace_hull(s.origin, end, DVec3::ZERO, Some(&filter)) {
        Some((_, hit)) if !hit.start_solid && hit.fraction.is_finite() => {
            s.origin.lerp(end, hit.fraction.clamp(0.0, 1.0))
        }
        _ => end,
    }
}

pub(crate) fn end_width(s: &Spotlight, distance: f64) -> f64 {
    (s.width * distance / s.length).clamp(0.0, MAX_BEAM_WIDTH)
}

pub(crate) fn dynamic_light(s: &Spotlight, end: DVec3) -> Option<PreviewLight> {
    if !s.on || !s.dynamic_light {
        return None;
    }
    let distance = s.origin.distance(end);
    let scale = LIGHT_SCALE * end_width(s, distance);
    let radius = (scale * LIGHT_RADIUS).min(f64::from(crate::lights::MAX_RANGE)) as f32;
    let strength = s.color.iter().fold(0.0f32, |a, b| a.max(*b));
    if radius < 1.0 || strength <= 0.0 {
        return None;
    }
    let back = end_width(s, distance).clamp(8.0, (distance * 0.5).max(1.0));
    let pos = end - s.dir * back.min(distance);
    Some(PreviewLight {
        kind: LightKind::Point,
        pos: pos.as_vec3().to_array(),
        color: s.color.map(|c| c / strength),
        intensity: strength,
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

pub(crate) fn beam_parts(
    s: &Spotlight,
    end: DVec3,
    srgb: [bool; 2],
    materials: [MaterialId; 2],
    object: ObjectId,
) -> Vec<WorldBeam> {
    if !s.on || s.origin.distance(end) < 1.0 {
        return Vec::new();
    }
    let start = s.origin.as_vec3().to_array();
    let tip = end.as_vec3().to_array();
    let beam = WorldBeam {
        object,
        material: materials[0],
        start,
        end: tip,
        width: s.width as f32,
        color: s.color.map(|c| c * BEAM_BRIGHTNESS),
        srgb_color: srgb[0],
        hdr_scale: s.hdr_scale,
        halo_scale: 0.0,
    };
    let halo =
        WorldBeam { material: materials[1], color: s.color, srgb_color: srgb[1], halo_scale: HALO_SCALE, ..beam };
    vec![beam, halo]
}

#[derive(Default)]
pub(crate) struct SpotlightProducer {
    ids: AHashSet<Id>,
    overrides: Vec<(Id, bool)>,
    waiting: bool,
    dirty: bool,
}

impl SpotlightProducer {
    fn overrides(cx: &ProducerCx<'_>) -> Vec<(Id, bool)> {
        let scene = cx.doc.scene();
        let mut out: Vec<(Id, bool)> = cx
            .assets
            .scene
            .projector_overrides
            .iter()
            .filter(|o| scene.entity(o.id).is_some_and(|n| is_spotlight(&n.entity.classname)))
            .filter_map(|o| Some((o.id, o.on?)))
            .collect();
        out.sort_unstable();
        out
    }
}

impl DerivedProducer for SpotlightProducer {
    fn name(&self) -> &'static str {
        "spotlights"
    }

    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        if changes.full || changes.entities || changes.doc.everything {
            self.dirty = true;
            return true;
        }
        if self.waiting
            && [BEAM_MATERIAL, HALO_MATERIAL].iter().all(|m| !matches!(cx.assets.mats.ana_look(m), AnaLook::Loading))
        {
            self.dirty = true;
        }
        let overrides = Self::overrides(cx);
        if overrides != self.overrides {
            self.dirty = true;
        }
        let scene = cx.doc.scene();
        let touches =
            |id: &Id| self.ids.contains(id) || scene.entity(*id).is_some_and(|n| is_spotlight(&n.entity.classname));
        if changes.doc.entities_changed.iter().any(touches)
            || changes.doc.entities_removed.iter().any(|id| self.ids.contains(id))
            || changes.doc.visibility_changed
            || (!self.ids.is_empty()
                && (!changes.doc.solids_changed.is_empty() || !changes.doc.solids_removed.is_empty()))
        {
            self.dirty = true;
        }
        self.dirty
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        self.dirty = false;
        self.ids.clear();
        self.overrides = Self::overrides(cx);
        let mut found: Vec<(Id, Spotlight)> = cx
            .doc
            .scene()
            .entities()
            .iter()
            .filter(|(id, n)| is_spotlight(&n.entity.classname) && cx.doc.is_entity_visible(**id))
            .filter_map(|(id, n)| {
                let over = self.overrides.iter().find(|(o, _)| o == id).map(|(_, on)| *on);
                Some((*id, spotlight(&n.entity, over)?))
            })
            .collect();
        found.sort_unstable_by_key(|(id, _)| *id);
        found.truncate(MAX_SPOTLIGHTS);
        let mut beams = Vec::new();
        let mut lights = Vec::new();
        let materials = [
            cx.assets.asset_material(BEAM_MATERIAL, MatKind::EntitySprite),
            cx.assets.asset_material(HALO_MATERIAL, MatKind::EntitySprite),
        ];
        let looks = [BEAM_MATERIAL, HALO_MATERIAL].map(|m| match cx.assets.mats.ana_look(m) {
            AnaLook::Ready(a) => Some(a.sprite_srgb),
            AnaLook::Missing => Some(true),
            AnaLook::Loading => None,
        });
        self.waiting = looks.iter().any(Option::is_none);
        let srgb = looks.map(|l| l.unwrap_or(true));
        for (id, s) in found {
            self.ids.insert(id);
            if !s.on {
                continue;
            }
            let end = beam_end(cx.doc, &s);
            beams.extend(beam_parts(&s, end, srgb, materials, entity_object(id)));
            if let Some(l) = dynamic_light(&s, end) {
                lights.push(SceneLight::same(l));
            }
        }
        DerivedBuild { beams: Some(beams), lights: Some(lights), ..DerivedBuild::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(keys: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: "point_spotlight".to_owned(), ..Default::default() };
        e.set("origin", "0 0 128");
        for (k, v) in keys {
            e.set(k, *v);
        }
        e
    }

    #[test]
    fn keys_follow_the_game_defaults() {
        let s = spotlight(&ent(&[]), None).expect("spotlight");
        assert_eq!((s.length, s.width, s.hdr_scale), (500.0, 50.0, 1.0));
        assert!(s.on && s.dynamic_light, "Start On is the FGD default when spawnflags is missing");
        assert_eq!(s.color, [1.0; 3]);
        assert!((s.dir - DVec3::X).length() < 1e-9);
        let down = spotlight(
            &ent(&[
                ("angles", "90 0 0"),
                ("spawnflags", "2"),
                ("rendercolor", "255 0 128"),
                ("SpotlightLength", "200"),
                ("SpotlightWidth", "20"),
                ("HDRColorScale", "2.5"),
            ]),
            None,
        )
        .expect("spotlight");
        assert!((down.dir - DVec3::NEG_Z).length() < 1e-9, "pitch 90 aims down");
        assert!(!down.on, "Start On unchecked");
        assert!(!down.dynamic_light);
        assert_eq!((down.length, down.width, down.hdr_scale), (200.0, 20.0, 2.5));
        assert!((down.color[2] - 128.0 / 255.0).abs() < 1e-6);
        assert!(spotlight(&ent(&[("spawnflags", "0")]), Some(true)).expect("on").on, "LightOn input");
        assert!(!spotlight(&ent(&[("spawnflags", "1")]), Some(false)).expect("off").on, "LightOff input");
    }

    #[test]
    fn the_light_sits_in_front_of_the_beam_end_and_obeys_no_dynamic_light() {
        let s = spotlight(&ent(&[("angles", "90 0 0")]), None).expect("spotlight");
        let end = DVec3::new(0.0, 0.0, 0.0);
        let l = dynamic_light(&s, end).expect("dynamic light");
        assert!(l.pos[2] > 0.0 && l.pos[2] < 128.0, "pulled back toward the spotlight: {:?}", l.pos);
        assert!(l.falloff.is_some_and(|f| f.dlight), "an engine dynamic light, not in the lightmaps");
        let w = end_width(&s, 128.0);
        assert!((f64::from(l.range) - w * LIGHT_SCALE * LIGHT_RADIUS).abs() < 1e-3);
        let off = Spotlight { dynamic_light: false, ..s.clone() };
        assert!(dynamic_light(&off, end).is_none());
        let dark = Spotlight { on: false, ..s };
        assert!(dynamic_light(&dark, end).is_none());
        assert!(beam_parts(&dark, end, [true; 2], [1, 2], 7).is_empty());
    }

    #[test]
    fn the_beam_is_one_engine_beam_and_a_halo() {
        let s = spotlight(&ent(&[("rendercolor", "255 128 0"), ("HDRColorScale", "2")]), None).expect("spotlight");
        let end = s.origin + s.dir * 300.0;
        let parts = beam_parts(&s, end, [true, false], [1, 2], 7);
        assert_eq!(parts.len(), 2);
        let (beam, halo) = (parts[0], parts[1]);
        assert_eq!((beam.material, halo.material, beam.object, halo.object), (1, 2, 7, 7));
        assert_eq!(beam.start, [0.0, 0.0, 128.0]);
        assert!((glam::Vec3::from(beam.end) - glam::Vec3::new(300.0, 0.0, 128.0)).length() < 1e-3);
        assert_eq!(beam.width, 50.0, "the beam keeps SpotlightWidth along its length");
        assert_eq!(beam.halo_scale, 0.0);
        assert!((beam.color[0] - 64.0 / 255.0).abs() < 1e-6, "brightness 64 of the engine beam");
        assert!((beam.color[1] - 128.0 / 255.0 * 64.0 / 255.0).abs() < 1e-6);
        assert_eq!(halo.halo_scale, HALO_SCALE);
        assert_eq!(halo.color, s.color, "the halo takes the full color");
        assert_eq!((beam.srgb_color, halo.srgb_color), (true, false));
        assert_eq!((beam.hdr_scale, halo.hdr_scale), (2.0, 2.0));
        let wide = spotlight(&ent(&[("SpotlightWidth", "500")]), None).expect("wide");
        assert!((wide.width - MAX_BEAM_WIDTH).abs() < 1e-9, "the engine clamps the width to 102.3");
        assert!(end_width(&s, 1.0e9) <= MAX_BEAM_WIDTH);
        assert!(beam_parts(&s, s.origin, [true; 2], [1, 2], 7).is_empty(), "no beam when blocked at once");
    }

    #[test]
    fn the_beam_stops_on_the_world() {
        let mut state = hp_app::testing::test_state();
        let params = hp_geom::PrimitiveParams {
            bounds: hp_geom::Aabb::new(DVec3::new(-64.0, -64.0, -16.0), DVec3::new(64.0, 64.0, 0.0)),
            ..Default::default()
        };
        state.doc.create_primitive(&params, Owner::World).expect("floor");
        let s = spotlight(&ent(&[("angles", "90 0 0")]), None).expect("spotlight");
        let end = beam_end(&state.doc, &s);
        assert!(end.z.abs() < 1e-6, "the beam ends on the floor: {end:?}");
        let up = Spotlight { dir: DVec3::Z, ..s };
        assert!((beam_end(&state.doc, &up) - DVec3::new(0.0, 0.0, 628.0)).length() < 1e-6);
    }
}
