use glam::{DMat3, DVec3};
use hp_geom::transform_angles_mat;
use hp_vmf::Entity;

const PITCHED_LIGHTS: [&str; 3] = ["light_spot", "light_environment", "light_directional"];

pub(super) fn is_pitched_light(e: &Entity) -> bool {
    let class = e.classname.trim();
    PITCHED_LIGHTS.iter().any(|c| class.eq_ignore_ascii_case(c))
}

fn key_num(e: &Entity, key: &str) -> Option<f64> {
    e.get(key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn fmt_num(v: f64) -> String {
    let mut s = String::new();
    hp_kv::fmt_num(&mut s, v + 0.0);
    s
}

pub(super) fn light_angles(e: &Entity) -> DVec3 {
    let a = e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO);
    let angle = key_num(e, "angle").unwrap_or(0.0);
    let (up, yaw) = if angle == -1.0 {
        (90.0, a.y)
    } else if angle == -2.0 {
        (-90.0, a.y)
    } else {
        let up = key_num(e, "pitch").filter(|p| *p != 0.0).unwrap_or(a.x);
        (up, if angle != 0.0 { angle } else { a.y })
    };
    DVec3::new(-up, yaw, a.z)
}

pub(super) fn transform_light(e: &mut Entity, linear: &DMat3) -> bool {
    let before = light_angles(e);
    let after = transform_angles_mat(before, linear);
    if !after.is_finite() {
        return false;
    }
    let up = -after.x + 0.0;
    let mut out: Vec<(&str, String)> = vec![("angles", hp_vmf::fmt_vec3(after)), ("pitch", fmt_num(up))];
    let angle = key_num(e, "angle").unwrap_or(0.0);
    if angle != 0.0 {
        let value = if up >= 90.0 {
            -1.0
        } else if up <= -90.0 {
            -2.0
        } else {
            let yaw = after.y.rem_euclid(360.0);
            if yaw == -1.0 || yaw == -2.0 { 0.0 } else { yaw }
        };
        out.push(("angle", fmt_num(value)));
    }
    let mut changed = false;
    for (k, v) in out {
        if e.get(k) != Some(v.as_str()) {
            e.set(k, v);
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DQuat;

    fn spot(kv: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: "light_spot".into(), ..Entity::default() };
        e.set("origin", "0 0 0");
        for (k, v) in kv {
            e.set(k, *v);
        }
        e
    }

    fn vrad_dir(e: &Entity) -> DVec3 {
        let a = e.angles().unwrap_or_default();
        let angle = key_num(e, "angle").unwrap_or(0.0);
        if angle == -1.0 {
            return DVec3::Z;
        }
        if angle == -2.0 {
            return DVec3::NEG_Z;
        }
        let yaw = if angle != 0.0 { angle } else { a.y };
        let pitch = key_num(e, "pitch").filter(|p| *p != 0.0).unwrap_or(a.x);
        let (sp, cp) = pitch.to_radians().sin_cos();
        let (sy, cy) = yaw.to_radians().sin_cos();
        DVec3::new(cp * cy, cp * sy, sp)
    }

    #[test]
    fn lights_turn_like_their_direction() {
        let cases: [&[(&str, &str)]; 5] = [
            &[("angles", "0 0 0"), ("pitch", "-90")],
            &[("angles", "0 30 0"), ("pitch", "-45")],
            &[("angles", "-20 45 0")],
            &[("angles", "45 90 0"), ("pitch", "-45")],
            &[("angles", "0 0 0"), ("angle", "120"), ("pitch", "-10")],
        ];
        for kv in cases {
            for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
                for deg in [90.0, -90.0, 37.0] {
                    let mut e = spot(kv);
                    let q = DQuat::from_axis_angle(axis, f64::to_radians(deg));
                    let before = vrad_dir(&e);
                    transform_light(&mut e, &DMat3::from_quat(q));
                    let after = vrad_dir(&e);
                    assert!((after - q * before).length() < 1e-6, "{kv:?} {axis} {deg}: {before} -> {after} {e:?}");
                    let model = hp_geom::angle_matrix(e.angles().expect("angles")).x_axis;
                    assert!((model - after).length() < 1e-6, "the model points where the light shines {e:?}");
                    let pitch = key_num(&e, "pitch").expect("pitch");
                    assert!((pitch + e.angles().expect("angles").x).abs() < 1e-9, "pitch is minus the angles pitch");
                }
            }
        }
    }

    #[test]
    fn straight_up_and_down_overrides() {
        let mut e = spot(&[("angle", "-2")]);
        transform_light(&mut e, &DMat3::from_rotation_z(f64::to_radians(90.0)));
        assert_eq!(e.get("angle"), Some("-2"));
        assert_eq!(e.get("pitch"), Some("-90"));
        transform_light(&mut e, &DMat3::from_rotation_y(f64::to_radians(-90.0)));
        assert!((vrad_dir(&e) - DVec3::X).length() < 1e-6, "{e:?}");
        let mut up = spot(&[("angle", "-2")]);
        transform_light(&mut up, &DMat3::from_rotation_x(f64::to_radians(180.0)));
        assert_eq!(up.get("angle"), Some("-1"));
        assert!(is_pitched_light(&up));
        assert!(!is_pitched_light(&Entity { classname: "prop_static".into(), ..Entity::default() }));
    }
}
