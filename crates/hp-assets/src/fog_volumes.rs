use hp_app::runtime::fog::{fog_volume_ids, resolve_volume};
use hp_editor::Document;
use hp_render::FogVolume;

pub(crate) use hp_app::runtime::fog::is_fog_volume;

pub(crate) fn fog_volumes(doc: &Document) -> Vec<FogVolume> {
    let scene = doc.scene();
    fog_volume_ids(doc)
        .into_iter()
        .filter(|id| scene.entity_solid_ids(*id).iter().any(|s| doc.is_solid_visible(*s)))
        .filter_map(|id| {
            let v = resolve_volume(doc, id)?;
            let (min, max) = v.bounds?;
            Some(FogVolume {
                min: min.as_vec3().to_array(),
                max: max.as_vec3().to_array(),
                fog: v.game_fog(),
                preview: v.preview,
                active: v.changes_game_fog() && !v.start_disabled,
            })
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
    use glam::DVec3;
    use hp_app::runtime::fog::DEFAULT_PREVIEW_FOG;
    use hp_editor::Id;
    use hp_vmf::{Entity, Map, Side, Solid, TexAxis};

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
        assert_eq!(f, v.preview);
        assert_eq!(f.color, [0.2, 0.4, 1.0]);
        assert_eq!(f.color2, [1.0, 0.0, 0.0]);
        assert_eq!((f.start, f.end, f.max_density), (64.0, 512.0, 0.75));
        assert!(f.blend);
        assert_eq!(f.dir, [0.0, 1.0, 0.0]);
    }

    #[test]
    fn unlinked_volumes_are_previewed_with_the_default_look() {
        for keys in [&[("FogName", "nothing")][..], &[]] {
            let list = fog_volumes(&doc(keys, CONTROLLER));
            assert_eq!(list.len(), 1, "{keys:?}");
            assert_eq!(list[0].preview, DEFAULT_PREVIEW_FOG);
            assert!(!list[0].active && list[0].fog.is_none(), "no effect in game");
        }
        assert!(fog_volumes(&doc(&[("FogName", "cave*")], CONTROLLER))[0].active);
    }

    #[test]
    fn disabled_fog_and_start_disabled() {
        let mut off: Vec<(&str, &str)> = CONTROLLER.to_vec();
        off[1] = ("fogenable", "0");
        let list = fog_volumes(&doc(&[("FogName", "cave_fog")], &off));
        assert_eq!(list.len(), 1, "the volume still overrides the fog");
        assert!(list[0].active && list[0].fog.is_none(), "with no fog in game");
        assert_eq!(list[0].preview, DEFAULT_PREVIEW_FOG, "but the preview shows it");
        let list = fog_volumes(&doc(&[("FogName", "cave_fog"), ("StartDisabled", "1")], CONTROLLER));
        assert!(!list[0].active);
    }

    #[test]
    fn hidden_volumes_are_left_out() {
        let mut d = doc(&[("FogName", "cave_fog")], CONTROLLER);
        assert_eq!(d.set_hidden(&[hp_editor::ObjRef::Entity(10)], true), 1);
        assert!(fog_volumes(&d).is_empty());
    }
}
