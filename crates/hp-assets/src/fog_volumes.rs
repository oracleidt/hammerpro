use glam::DVec3;
use hp_editor::{Document, Id};
use hp_render::{Fog, FogVolume};
use hp_vmf::Entity;

use crate::util::parse_floats;

pub(crate) fn is_fog_volume(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case("fog_volume")
}

pub(crate) fn is_fog_controller(class: &str) -> bool {
    class.trim().eq_ignore_ascii_case("env_fog_controller")
}

fn color_key(e: &Entity, key: &str, fallback: [f32; 3]) -> [f32; 3] {
    match e.get(key).map(parse_floats::<3>) {
        Some((c, 3)) if c.iter().all(|v| v.is_finite()) => c.map(|v| (v / 255.0).clamp(0.0, 1.0) as f32),
        _ => fallback,
    }
}

fn num(e: &Entity, key: &str, fallback: f32) -> f32 {
    e.get(key).and_then(|s| s.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(fallback)
}

fn flag(e: &Entity, key: &str) -> bool {
    num(e, key, 0.0) != 0.0
}

fn angles_forward(a: DVec3) -> [f32; 3] {
    let (p, y) = (a.x.to_radians(), a.y.to_radians());
    [(p.cos() * y.cos()) as f32, (p.cos() * y.sin()) as f32, (-p.sin()) as f32]
}

pub(crate) fn controller_fog(e: &Entity) -> Fog {
    let color = color_key(e, "fogcolor", [1.0; 3]);
    let dir = if flag(e, "use_angles") {
        angles_forward(e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO))
    } else {
        match e.get("fogdir").map(parse_floats::<3>) {
            Some((d, 3)) if d.iter().all(|v| v.is_finite()) => d.map(|v| v as f32),
            _ => [1.0, 0.0, 0.0],
        }
    };
    let start = num(e, "fogstart", 0.0).max(0.0);
    Fog {
        color,
        color2: color_key(e, "fogcolor2", color),
        dir,
        blend: flag(e, "fogblend"),
        start,
        end: num(e, "fogend", 4096.0).max(start + 1.0),
        max_density: num(e, "fogmaxdensity", 1.0).clamp(0.0, 1.0),
    }
}

pub(crate) fn name_matches(pattern: &str, name: &str) -> bool {
    let (pattern, name) = (pattern.trim(), name.trim());
    if pattern.is_empty() || name.is_empty() {
        return false;
    }
    match pattern.strip_suffix('*') {
        Some(prefix) => name.len() >= prefix.len() && name[..prefix.len()].eq_ignore_ascii_case(prefix),
        None => name.eq_ignore_ascii_case(pattern),
    }
}

pub(crate) fn find_controller<'a>(doc: &'a Document, fog_name: &str) -> Option<&'a Entity> {
    let scene = doc.scene();
    let mut found: Vec<(&Id, &Entity)> = scene
        .entities()
        .iter()
        .map(|(id, n)| (id, &n.entity))
        .filter(|(_, e)| is_fog_controller(&e.classname))
        .filter(|(_, e)| e.get("targetname").is_some_and(|t| name_matches(fog_name, t)))
        .collect();
    found.sort_unstable_by_key(|(id, _)| **id);
    found.first().map(|(_, e)| *e)
}

pub(crate) fn volume_fog(doc: &Document, volume: &Entity) -> Option<Option<Fog>> {
    let name = volume.get("FogName").map(str::trim).filter(|n| !n.is_empty())?;
    let controller = find_controller(doc, name)?;
    Some(flag(controller, "fogenable").then(|| controller_fog(controller)))
}

pub(crate) fn fog_volumes(doc: &Document) -> Vec<FogVolume> {
    let scene = doc.scene();
    let mut ids: Vec<Id> =
        scene.entities().iter().filter(|(_, n)| is_fog_volume(&n.entity.classname)).map(|(id, _)| *id).collect();
    ids.sort_unstable();
    ids.into_iter()
        .filter_map(|id| {
            let e = &scene.entity(id)?.entity;
            if !scene.entity_solid_ids(id).iter().any(|s| doc.is_solid_visible(*s)) {
                return None;
            }
            let bounds = scene.entity_solid_bounds(id)?;
            let fog = volume_fog(doc, e)?;
            let (min, max) = (bounds.min.as_vec3().to_array(), bounds.max.as_vec3().to_array());
            Some(FogVolume { min, max, fog, active: !flag(e, "StartDisabled") })
        })
        .collect()
}

impl crate::AssetManager {
    pub(crate) fn sync_fog_volumes(
        &mut self,
        doc: &Document,
        renderer: &mut hp_render::Renderer,
        display: &hp_app::runtime::render::RenderDisplay,
        dirty: bool,
    ) {
        let on = display.fog_volumes;
        if dirty || self.scene.env.fog_volumes_on != Some(on) {
            self.scene.env.fog_volumes_on = Some(on);
            let list = if on { fog_volumes(doc) } else { Vec::new() };
            renderer.set_fog_volumes(&list);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_vmf::{Map, Side, Solid, TexAxis};

    fn ent(id: Id, class: &str, props: &[(&str, &str)]) -> Entity {
        let mut e = Entity { id, classname: class.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    pub(crate) fn block(id: Id, lo: DVec3, hi: DVec3, material: &str) -> Solid {
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

    fn doc(volume: &[(&str, &str)], controller: &[(&str, &str)]) -> Document {
        let mut m = Map::new_empty();
        let mut v = ent(10, "fog_volume", volume);
        v.solids.push(block(11, DVec3::new(-64.0, -32.0, 0.0), DVec3::new(64.0, 32.0, 128.0), "tools/toolstrigger"));
        v.solids.push(block(12, DVec3::new(64.0, -32.0, 0.0), DVec3::new(256.0, 32.0, 64.0), "tools/toolstrigger"));
        m.entities.push(v);
        m.entities.push(ent(20, "env_fog_controller", controller));
        m.entities.push(ent(30, "env_fog_controller", &[("targetname", "other"), ("fogenable", "1")]));
        Document::from_map(m)
    }

    const CONTROLLER: &[(&str, &str)] = &[
        ("targetname", "Cave_Fog"),
        ("fogenable", "1"),
        ("fogcolor", "51 102 255"),
        ("fogcolor2", "255 0 0"),
        ("fogstart", "64"),
        ("fogend", "512"),
        ("fogmaxdensity", "0.75"),
        ("fogblend", "1"),
        ("fogdir", "0 1 0"),
    ];

    #[test]
    fn the_volume_takes_the_fog_of_its_controller() {
        let d = doc(&[("FogName", "cave_fog")], CONTROLLER);
        let list = fog_volumes(&d);
        assert_eq!(list.len(), 1);
        let v = list[0];
        assert_eq!((v.min, v.max), ([-64.0, -32.0, 0.0], [256.0, 32.0, 128.0]), "the box around every brush");
        assert!(v.active);
        let f = v.fog.expect("fog on");
        assert_eq!(f.color, [0.2, 0.4, 1.0]);
        assert_eq!(f.color2, [1.0, 0.0, 0.0]);
        assert_eq!((f.start, f.end, f.max_density), (64.0, 512.0, 0.75));
        assert!(f.blend);
        assert_eq!(f.dir, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn names_wildcards_and_missing_controllers() {
        assert!(name_matches("cave*", "CAVE_fog") && name_matches("Cave_Fog", "cave_fog"));
        assert!(!name_matches("cave", "cave_fog") && !name_matches("", "a") && !name_matches("*x", "a"));
        assert_eq!(fog_volumes(&doc(&[("FogName", "cave*")], CONTROLLER)).len(), 1);
        assert!(fog_volumes(&doc(&[("FogName", "nothing")], CONTROLLER)).is_empty(), "no such controller");
        assert!(fog_volumes(&doc(&[], CONTROLLER)).is_empty(), "no FogName");
    }

    #[test]
    fn disabled_fog_and_start_disabled() {
        let mut off: Vec<(&str, &str)> = CONTROLLER.to_vec();
        off[1] = ("fogenable", "0");
        let list = fog_volumes(&doc(&[("FogName", "cave_fog")], &off));
        assert_eq!(list.len(), 1, "the volume still overrides the fog");
        assert!(list[0].fog.is_none(), "with no fog");
        let list = fog_volumes(&doc(&[("FogName", "cave_fog"), ("StartDisabled", "1")], CONTROLLER));
        assert!(!list[0].active);
    }

    #[test]
    fn controller_keys_are_sanitized() {
        let f = controller_fog(&ent(
            1,
            "env_fog_controller",
            &[("fogstart", "-50"), ("fogend", "nan"), ("fogmaxdensity", "4"), ("fogcolor", "a b c")],
        ));
        assert_eq!((f.start, f.end, f.max_density), (0.0, 4096.0, 1.0));
        assert_eq!(f.color, [1.0; 3]);
        assert_eq!(f.color2, f.color);
        let angled = controller_fog(&ent(
            1,
            "env_fog_controller",
            &[("use_angles", "1"), ("angles", "-90 0 0"), ("fogdir", "1 0 0")],
        ));
        assert!((angled.dir[2] - 1.0).abs() < 1e-6, "pitch -90 looks up: {:?}", angled.dir);
    }

    #[test]
    fn hidden_volumes_are_left_out() {
        let mut d = doc(&[("FogName", "cave_fog")], CONTROLLER);
        assert_eq!(d.set_hidden(&[hp_editor::ObjRef::Entity(10)], true), 1);
        assert!(fog_volumes(&d).is_empty());
    }
}
