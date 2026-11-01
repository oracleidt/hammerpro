use glam::DVec3;
use hp_geom::{Aabb, Ray};
use hp_vmf::Map;
use hp_vmf::{Entity, Id};

use crate::{Document, EditError, EditResult, IdSet, ObjRef, Owner, PickOptions};

pub const SKY_CAMERA: &str = "sky_camera";
pub const SWITCH_CLASS: &str = "point_hammerpro_skybox";
pub const SWITCH_LUA_PATH: &str = "lua/entities/point_hammerpro_skybox.lua";
pub const SWITCH_LUA: &str = include_str!("../../../assets/skybox/point_hammerpro_skybox.lua");
pub const SWITCH_ENGINE_PATH: &str = "lua/autorun/hammerpro_skybox.lua";
pub const SWITCH_ENGINE_LUA: &str = include_str!("../../../assets/skybox/hammerpro_skybox.lua");

pub fn switch_lua_files() -> [(&'static str, &'static str); 2] {
    [(SWITCH_LUA_PATH, SWITCH_LUA), (SWITCH_ENGINE_PATH, SWITCH_ENGINE_LUA)]
}
pub const BUILTIN_FGD: &str = include_str!("../../../assets/skybox/hammerpro.fgd");
pub const SWITCH_KEY_PREFIX: &str = "sky_";
pub const DEFAULT_SKY_SCALE: f32 = 16.0;
pub const MAX_SKY_SCALE: f32 = 1024.0;
const SKY_ROOM_REACH: f64 = 32768.0;

#[derive(Clone, Debug, PartialEq)]
pub struct SkyCamera {
    pub id: Id,
    pub name: String,
    pub origin: DVec3,
    pub scale: f32,
    pub scale_valid: bool,
    pub scale_text: String,
}

pub fn is_sky_camera(e: &Entity) -> bool {
    e.classname.trim().eq_ignore_ascii_case(SKY_CAMERA)
}

pub fn parse_sky_scale(text: &str) -> Option<f32> {
    let t = text.trim();
    if t.is_empty() {
        return Some(DEFAULT_SKY_SCALE);
    }
    let v: f64 = t.parse().ok()?;
    (v.is_finite() && v >= 1.0 && v.fract() == 0.0).then_some(v.min(f64::from(MAX_SKY_SCALE)) as f32)
}

pub fn sky_camera_of(e: &Entity) -> Option<SkyCamera> {
    if !is_sky_camera(e) {
        return None;
    }
    let origin = e.origin().filter(|o| o.is_finite())?;
    let scale_text = e.get("scale").unwrap_or_default().trim().to_owned();
    let parsed = parse_sky_scale(&scale_text);
    Some(SkyCamera {
        id: e.id,
        name: e.get("targetname").unwrap_or_default().trim().to_owned(),
        origin,
        scale: parsed.unwrap_or(DEFAULT_SKY_SCALE),
        scale_valid: parsed.is_some(),
        scale_text,
    })
}

pub fn sky_cameras(doc: &Document) -> Vec<SkyCamera> {
    doc.scene().entities().values().filter_map(|n| sky_camera_of(&n.entity)).collect()
}

pub fn game_sky_camera(doc: &Document) -> Option<Id> {
    doc.scene().entities().values().rev().find_map(|n| sky_camera_of(&n.entity)).map(|c| c.id)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkyRoom {
    pub bounds: Aabb,
    pub solids: IdSet,
    pub entities: IdSet,
}

fn is_sky_solid(doc: &Document, id: Id) -> bool {
    doc.scene()
        .solid(id)
        .is_some_and(|n| n.solid.sides.iter().any(|s| s.material.to_ascii_lowercase().contains("toolsskybox")))
}

pub fn find_sky_room(doc: &Document, origin: DVec3) -> Option<SkyRoom> {
    if !origin.is_finite() {
        return None;
    }
    let sky_only = |o: ObjRef| matches!(o, ObjRef::Solid(id) if is_sky_solid(doc, id));
    let world_only = |o: ObjRef| matches!(o, ObjRef::Solid(id) if doc.scene().solid(id).is_some_and(|n| matches!(n.owner, Owner::World)));
    let no_bounds = |_: &Entity| None;
    let hit = |dir: DVec3, filter: &dyn Fn(ObjRef) -> bool| {
        let opts = PickOptions {
            include_solids: true,
            include_entities: false,
            ignore_hidden: false,
            max_distance: SKY_ROOM_REACH,
            filter: Some(filter),
            ..PickOptions::default()
        };
        doc.pick_ray(&Ray::new(origin, dir), &opts, &no_bounds).and_then(|h| match h.obj {
            ObjRef::Solid(id) => doc.scene().solid(id).map(|n| n.bounds),
            ObjRef::Entity(_) => None,
        })
    };
    let mut bounds = Aabb::new(origin, origin);
    for axis in 0..3 {
        for sign in [-1.0, 1.0] {
            let mut dir = DVec3::ZERO;
            dir[axis] = sign;
            let b = hit(dir, &sky_only).or_else(|| hit(dir, &world_only)).filter(|b| !b.is_empty())?;
            bounds.extend(b.min);
            bounds.extend(b.max);
        }
    }
    let room = Aabb::new(bounds.min - DVec3::ONE, bounds.max + DVec3::ONE);
    let scene = doc.scene();
    let solids: IdSet = doc
        .solids_in_box(&room)
        .into_iter()
        .filter(|id| scene.solid(*id).is_some_and(|n| !n.bounds.is_empty() && room.contains_point(n.bounds.center())))
        .collect();
    let entities: IdSet = scene
        .entities()
        .iter()
        .filter(|(id, n)| {
            if scene.is_brush_entity(**id) {
                let ids = scene.entity_solid_ids(**id);
                !ids.is_empty() && ids.iter().all(|s| solids.contains(s))
            } else {
                n.entity.origin().is_some_and(|o| room.contains_point(o))
            }
        })
        .map(|(id, _)| *id)
        .collect();
    Some(SkyRoom { bounds: room, solids, entities })
}

pub fn is_skybox_switch(e: &Entity) -> bool {
    e.classname.trim().eq_ignore_ascii_case(SWITCH_CLASS)
}

pub fn switch_target(e: &Entity) -> &str {
    e.get("skycamera").unwrap_or_default().trim()
}

pub fn map_uses_switches(map: &Map) -> bool {
    map.entities.iter().any(|e| !e.hidden && is_skybox_switch(e))
}

fn named_camera<'a>(entities: &'a [Entity], name: &str) -> Option<&'a Entity> {
    if name.is_empty() {
        return None;
    }
    entities
        .iter()
        .rev()
        .find(|e| is_sky_camera(e) && e.get("targetname").is_some_and(|n| n.trim().eq_ignore_ascii_case(name)))
}

pub fn bake_switches(map: &mut Map) -> usize {
    let cameras: Vec<(String, Vec<(String, String)>)> = map
        .entities
        .iter()
        .filter(|e| is_skybox_switch(e))
        .filter_map(|e| {
            let cam = named_camera(&map.entities, switch_target(e))?;
            let keys = cam
                .props
                .iter()
                .filter(|(k, _)| {
                    !matches!(k.to_ascii_lowercase().as_str(), "classname" | "targetname" | "id" | "hammerid")
                })
                .map(|(k, v)| (format!("{SWITCH_KEY_PREFIX}{}", k.to_ascii_lowercase()), v.clone()))
                .collect();
            Some((switch_target(e).to_ascii_lowercase(), keys))
        })
        .collect();
    let mut baked = 0;
    for e in map.entities.iter_mut().filter(|e| is_skybox_switch(e)) {
        e.props.retain(|(k, _)| !k.to_ascii_lowercase().starts_with(SWITCH_KEY_PREFIX));
        let target = switch_target(e).to_ascii_lowercase();
        if let Some((_, keys)) = cameras.iter().find(|(t, _)| *t == target) {
            e.props.extend(keys.iter().cloned());
            baked += 1;
        }
    }
    baked
}

impl Document {
    pub fn make_sky_camera_active(&mut self, id: Id) -> EditResult<bool> {
        let node = self.scene.entities.get(&id).ok_or(EditError::NotFound(ObjRef::Entity(id)))?;
        if !is_sky_camera(&node.entity) {
            return Err(EditError::Invalid(format!("entity {id} is not a sky_camera")));
        }
        if game_sky_camera(self) == Some(id) {
            return Ok(false);
        }
        let mut node = node.clone();
        self.tx("Use sky camera in game", |doc| {
            node.order = doc.scene.alloc_order();
            doc.write_entities(vec![(id, Some(node))]);
            Ok(true)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_vmf::{Map, Side, Solid, TexAxis};

    fn block(id: Id, lo: DVec3, hi: DVec3, material: &str) -> Solid {
        let p = DVec3::new;
        let planes = [
            [p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z)],
            [p(lo.x, lo.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)],
            [p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)],
            [p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z)],
            [p(hi.x, hi.y, hi.z), p(lo.x, hi.y, hi.z), p(lo.x, hi.y, lo.z)],
            [p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z)],
        ];
        let sides = planes
            .iter()
            .enumerate()
            .map(|(k, plane)| Side {
                id: id * 10 + k as Id,
                plane: *plane,
                material: material.to_owned(),
                uaxis: TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
                vaxis: TexAxis { axis: DVec3::NEG_Y, shift: 0.0, scale: 0.25 },
                ..Default::default()
            })
            .collect();
        Solid { id, sides, ..Default::default() }
    }

    fn room(first: Id, lo: DVec3, hi: DVec3) -> Vec<Solid> {
        let (t, m) = (16.0, "TOOLS/TOOLSSKYBOX");
        let v = DVec3::new;
        vec![
            block(first, v(lo.x - t, lo.y - t, lo.z - t), v(hi.x + t, hi.y + t, lo.z), "nature/dirt"),
            block(first + 1, v(lo.x - t, lo.y - t, hi.z), v(hi.x + t, hi.y + t, hi.z + t), m),
            block(first + 2, v(lo.x - t, lo.y - t, lo.z), v(lo.x, hi.y + t, hi.z), m),
            block(first + 3, v(hi.x, lo.y - t, lo.z), v(hi.x + t, hi.y + t, hi.z), m),
            block(first + 4, v(lo.x, lo.y - t, lo.z), v(hi.x, lo.y, hi.z), m),
            block(first + 5, v(lo.x, hi.y, lo.z), v(hi.x, hi.y + t, hi.z), m),
        ]
    }

    fn ent(id: Id, class: &str, props: &[(&str, &str)]) -> Entity {
        let mut e = Entity { id, classname: class.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    fn two_skyboxes() -> Document {
        let mut m = Map::new_empty();
        let v = DVec3::new;
        m.world.solids.extend(room(100, v(-512.0, -512.0, 2000.0), v(512.0, 512.0, 2512.0)));
        m.world.solids.extend(room(200, v(2000.0, -512.0, 2000.0), v(3024.0, 512.0, 2512.0)));
        m.world.solids.push(block(300, v(-64.0, -64.0, 2000.0), v(64.0, 64.0, 2200.0), "brick/a"));
        m.entities.push(ent(1, "sky_camera", &[("origin", "0 0 2300"), ("scale", "16"), ("targetname", "day")]));
        m.entities.push(ent(2, "prop_static", &[("origin", "100 100 2001")]));
        m.entities.push(ent(3, "SKY_CAMERA", &[("origin", "2500 0 2300"), ("scale", "32"), ("targetname", "night")]));
        m.entities.push(ent(4, "info_player_start", &[("origin", "0 0 0")]));
        Document::from_map(m)
    }

    #[test]
    fn scales_are_whole_numbers_of_at_least_one() {
        assert_eq!(parse_sky_scale(""), Some(DEFAULT_SKY_SCALE));
        assert_eq!(parse_sky_scale(" 16 "), Some(16.0));
        assert_eq!(parse_sky_scale("16.0"), Some(16.0));
        assert_eq!(parse_sky_scale("1e9"), Some(MAX_SKY_SCALE));
        for bad in ["0", "-16", "0.5", "16.5", "abc", "NaN", "inf"] {
            assert_eq!(parse_sky_scale(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_game_uses_the_last_sky_camera() {
        let mut doc = two_skyboxes();
        let cams = sky_cameras(&doc);
        assert_eq!(
            cams.iter().map(|c| (c.id, c.name.as_str(), c.scale)).collect::<Vec<_>>(),
            [(1, "day", 16.0), (3, "night", 32.0)]
        );
        assert_eq!(game_sky_camera(&doc), Some(3));
        assert!(doc.make_sky_camera_active(1).expect("sky camera"));
        assert_eq!(game_sky_camera(&doc), Some(1));
        let order: Vec<Id> = doc.scene().entities().keys().copied().collect();
        assert_eq!(order, [2, 3, 4, 1]);
        assert_eq!(doc.undo_label(), Some("Use sky camera in game"));
        assert!(!doc.make_sky_camera_active(1).expect("already"), "no step when it already is");
        assert!(doc.undo());
        assert_eq!(game_sky_camera(&doc), Some(3));
        assert!(doc.make_sky_camera_active(4).is_err(), "not a sky camera");
        assert!(doc.make_sky_camera_active(999).is_err());
        assert!(doc.make_sky_camera_active(1).expect("sky camera"));
        let ids: Vec<Id> = doc.scene().to_map().entities.iter().map(|e| e.id).collect();
        assert_eq!(ids, [2, 3, 4, 1]);
    }

    #[test]
    fn switches_get_the_keys_of_their_sky_camera() {
        let mut map = two_skyboxes().to_map();
        let mut switch = ent(10, SWITCH_CLASS, &[("skycamera", " Night "), ("targetname", "to_night")]);
        switch.set("sky_stale", "1");
        map.entities.push(switch);
        map.entities.push(ent(11, SWITCH_CLASS, &[("skycamera", "nothing")]));
        assert!(map_uses_switches(&map));
        assert_eq!(bake_switches(&mut map), 1);
        let s = map.entities.iter().find(|e| e.id == 10).expect("switch");
        assert_eq!(s.get("sky_origin"), Some("2500 0 2300"));
        assert_eq!(s.get("sky_scale"), Some("32"));
        assert_eq!(s.get("sky_stale"), None, "old baked keys are replaced");
        assert_eq!(s.get("sky_targetname"), None);
        assert_eq!(s.get("targetname"), Some("to_night"));
        let missing = map.entities.iter().find(|e| e.id == 11).expect("switch");
        assert!(missing.props.iter().all(|(k, _)| !k.starts_with("sky_")));
        let mut plain = Map::new_empty();
        assert!(!map_uses_switches(&plain));
        assert_eq!(bake_switches(&mut plain), 0);
    }

    #[test]
    fn per_player_switches_keep_their_mode_key() {
        let mut map = two_skyboxes().to_map();
        map.entities.push(ent(10, SWITCH_CLASS, &[("skycamera", "night"), ("mode", "1")]));
        bake_switches(&mut map);
        let s = map.entities.iter().find(|e| e.id == 10).unwrap();
        assert_eq!(s.get("mode"), Some("1"), "the per player mode survives baking");
        assert_eq!(s.get("sky_origin"), Some("2500 0 2300"));
    }

    #[test]
    fn the_builtin_files_are_there() {
        assert!(SWITCH_LUA.contains("ENT.Type = \"point\""));
        assert!(SWITCH_ENGINE_LUA.contains("ents.Create(\"sky_camera\")"));
        assert!(SWITCH_ENGINE_LUA.contains("PreDrawSkyBox"), "the per player render is there");
        assert!(SWITCH_ENGINE_LUA.contains("PreRender"), "the render target pass is there");
        assert!(BUILTIN_FGD.contains(SWITCH_CLASS));
        assert!(SWITCH_LUA_PATH.ends_with(&format!("{SWITCH_CLASS}.lua")));
        assert_eq!(switch_lua_files().len(), 2);
        assert!(switch_lua_files().iter().all(|(p, c)| p.starts_with("lua/") && !c.is_empty()));
    }

    #[test]
    fn each_sky_camera_has_its_room() {
        let doc = two_skyboxes();
        let day = find_sky_room(&doc, DVec3::new(0.0, 0.0, 2300.0)).expect("day room");
        assert!(day.solids.contains(&300) && day.entities.contains(&1) && day.entities.contains(&2));
        assert!(!day.entities.contains(&3) && !day.solids.contains(&201));
        let night = find_sky_room(&doc, DVec3::new(2500.0, 0.0, 2300.0)).expect("night room");
        assert!(night.entities.contains(&3) && !night.solids.contains(&300));
        assert!(find_sky_room(&doc, DVec3::new(0.0, 0.0, -5000.0)).is_none(), "open space");
        assert!(find_sky_room(&doc, DVec3::NAN).is_none());
    }
}
