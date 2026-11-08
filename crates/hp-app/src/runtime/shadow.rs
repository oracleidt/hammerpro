use glam::DVec3;
use hp_render::ShadowControl;

pub fn preview(state: &crate::AppState) -> Option<ShadowControl> {
    let options = state.doc.light_plus();
    let (_, node) = state
        .doc
        .scene()
        .entities()
        .iter()
        .filter(|(id, n)| {
            state.doc.is_entity_visible(**id)
                && options.uses_entity(**id)
                && n.entity.classname.eq_ignore_ascii_case("shadow_control")
        })
        .max_by_key(|(id, _)| **id)?;
    let e = &node.entity;
    let number = |key: &str, default: f32| {
        e.get(key).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
    };
    if number("disableallshadows", 0.0) != 0.0 {
        return None;
    }
    let direction = if let Some(angles) = e.angles().filter(|a| a.is_finite()) {
        let angles =
            if angles == DVec3::ZERO { DVec3::new(80.0, 30.0, 0.0) } else { angles } * (std::f64::consts::PI / 180.0);
        DVec3::new(angles.x.cos() * angles.y.cos(), angles.x.cos() * angles.y.sin(), -angles.x.sin())
    } else {
        e.get("direction").and_then(hp_vmf::parse_vec3).filter(|d| d.is_finite()).unwrap_or(DVec3::new(0.2, 0.2, -2.0))
    };
    let mut color = [64.0 / 255.0; 3];
    for (dst, src) in color.iter_mut().zip(e.get("color").unwrap_or("64 64 64").split_whitespace()) {
        *dst = src.parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(64.0).clamp(0.0, 255.0) / 255.0;
    }
    Some(ShadowControl {
        direction: direction.normalize_or_zero().as_vec3().to_array(),
        color,
        distance: number("distance", 50.0).clamp(0.0, 65536.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    #[test]
    fn controller_properties_and_visibility_drive_the_preview() {
        let mut state = crate::testing::test_state();
        assert!(preview(&state).is_none());
        let mut map = hp_vmf::Map::new_empty();
        let mut entity = hp_vmf::Entity { id: 900, classname: "shadow_control".into(), ..Default::default() };
        entity.set("angles", "90 0 0");
        entity.set("color", "255 0 128");
        entity.set("distance", "320");
        map.entities.push(entity);
        state.doc = hp_editor::Document::from_map(map);
        let control = preview(&state).unwrap();
        assert!(Vec3::from_array(control.direction).distance(Vec3::NEG_Z) < 1e-5);
        assert_eq!(control.color, [1.0, 0.0, 128.0 / 255.0]);
        assert_eq!(control.distance, 320.0);
        state.doc.set_entity_key(&[900], "disableallshadows", "1").unwrap();
        assert!(preview(&state).is_none());
        state.doc.set_entity_key(&[900], "disableallshadows", "0").unwrap();
        state.doc.set_hidden(&[hp_editor::ObjRef::Entity(900)], true);
        assert!(preview(&state).is_none());
    }
}
