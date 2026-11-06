use glam::{DMat4, DVec3};
use hp_editor::limits::world::{ParticleVolumes, particle_precipitation};

pub fn read(bsp: &hp_bsp::Bsp, entities: &[hp_bsp::entities::Entity]) -> ParticleVolumes {
    let entities: Vec<_> = entities
        .iter()
        .filter(|entity| {
            matches!(
                entity.classname().to_ascii_lowercase().as_str(),
                "func_precipitation" | "func_precipitation_blocker"
            )
        })
        .map(|entity| hp_vmf::Entity {
            classname: entity.classname().to_owned(),
            props: entity.pairs.clone(),
            ..Default::default()
        })
        .collect();
    if !entities.iter().any(particle_precipitation) {
        return ParticleVolumes::default();
    }
    let Ok(physics) = bsp.lump(29) else { return ParticleVolumes::default() };
    decode(&physics, &entities)
}

fn decode(physics: &[u8], entities: &[hp_vmf::Entity]) -> ParticleVolumes {
    let mut out = ParticleVolumes::default();
    for entity in entities {
        let blocker = entity.classname.eq_ignore_ascii_case("func_precipitation_blocker");
        if !blocker && !particle_precipitation(entity) {
            continue;
        }
        let Some(model) =
            entity.get("model").and_then(|value| value.trim().strip_prefix('*')).and_then(|value| value.parse().ok())
        else {
            continue;
        };
        let Some(data) = hp_bsp::physics::model_as_phy(physics, model) else { continue };
        let Ok(collision) = hp_mdl::parse_phy(&data) else { continue };
        let origin = entity.origin().unwrap_or(DVec3::ZERO);
        let angles = entity.angles().unwrap_or(DVec3::ZERO);
        let transform = DMat4::from_rotation_translation(hp_geom::angles_to_quat(angles), origin);
        let target = if blocker { &mut out.blockers } else { &mut out.precipitation };
        for solid in collision.solids {
            for triangle in solid.piece_triangles() {
                let points = triangle.map(|index| {
                    transform
                        .transform_point3(DVec3::from(solid.vertices[index as usize].map(f64::from)))
                        .as_vec3()
                        .to_array()
                });
                if points.iter().flatten().all(|value| value.is_finite()) {
                    target.push([points[0], points[2], points[1]]);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const NATIVE: &[u8] =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../hp-pcf/tests/data/precip_physics.lump"));
    fn entity(class: &str, model: &str) -> hp_vmf::Entity {
        let mut entity = hp_vmf::Entity { classname: class.into(), ..Default::default() };
        entity.set("model", model);
        entity.set("preciptype", "4");
        entity
    }
    #[test]
    fn native_cooked_volume_geometry_and_inward_normals_are_preserved() {
        let volumes = decode(NATIVE, &[entity("func_precipitation", "*1"), entity("func_precipitation_blocker", "*2")]);
        for (triangles, lo, hi) in [
            (&volumes.precipitation, [-127.5, -127.5, 16.5], [127.5, 127.5, 255.5]),
            (&volumes.blockers, [-63.5, -63.5, 128.5], [63.5, 63.5, 143.5]),
        ] {
            assert_eq!(triangles.len(), 12);
            let center = (DVec3::from(lo) + DVec3::from(hi)) * 0.5;
            let mut minimum = DVec3::splat(f64::INFINITY);
            let mut maximum = DVec3::splat(f64::NEG_INFINITY);
            for triangle in triangles {
                let [a, b, c] = triangle.map(|point| DVec3::from(point.map(f64::from)));
                assert!((b - a).cross(c - a).dot(center - a) > 0.0);
                for point in [a, b, c] {
                    minimum = minimum.min(point);
                    maximum = maximum.max(point);
                }
            }
            assert_eq!(minimum, DVec3::from(lo));
            assert_eq!(maximum, DVec3::from(hi));
        }
    }
    #[test]
    fn legacy_precipitation_is_not_added_to_particle_ray_geometry() {
        let mut legacy = entity("func_precipitation", "*1");
        legacy.set("preciptype", "0");
        assert!(decode(NATIVE, &[legacy]).precipitation.is_empty());
    }
}
