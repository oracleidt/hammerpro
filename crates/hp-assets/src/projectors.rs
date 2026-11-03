use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ahash::AHashMap;
use glam::DVec3;
use hp_editor::Document;
use hp_render::{Projector, ProjectorImage};
use hp_vmf::Entity;

pub(crate) const PROJECTED_TEXTURE: &str = "env_projectedtexture";
const DEFAULT_TEXTURE: &str = "effects/flashlight001";
const START_DISABLED: i64 = 4;
const MAX_FOV: f64 = 170.0;
const DEFAULT_FOV: f64 = 45.0;

pub(crate) const BRIGHTNESS: f32 = 0.567;
pub(crate) const ATTENUATION: [f32; 3] = [0.0, 100.0, 0.0];

fn key_f(e: &Entity, key: &str) -> Option<f64> {
    e.get(key).and_then(|s| s.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

pub(crate) fn angle_vectors(angles: DVec3) -> (DVec3, DVec3, DVec3) {
    let (sp, cp) = angles.x.to_radians().sin_cos();
    let (sy, cy) = angles.y.to_radians().sin_cos();
    let (sr, cr) = angles.z.to_radians().sin_cos();
    let forward = DVec3::new(cp * cy, cp * sy, -sp);
    let right = DVec3::new(-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp);
    let up = DVec3::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
    (forward, right, up)
}

pub(crate) fn basis_towards(direction: DVec3) -> (DVec3, DVec3, DVec3) {
    let forward = direction.try_normalize().unwrap_or(DVec3::X);
    let pitch = (-forward.z).clamp(-1.0, 1.0).asin().to_degrees();
    let yaw =
        if forward.x.abs() < 1e-9 && forward.y.abs() < 1e-9 { 0.0 } else { forward.y.atan2(forward.x).to_degrees() };
    angle_vectors(DVec3::new(pitch, yaw, 0.0))
}

pub(crate) fn light_color(e: &Entity) -> [f32; 3] {
    let text = e.get("lightcolor").unwrap_or("255 255 255");
    let mut c = [255.0f32; 3];
    for (slot, word) in c.iter_mut().zip(text.split_whitespace()) {
        if let Ok(v) = word.parse::<f32>()
            && v.is_finite()
        {
            *slot = v;
        }
    }
    let strength = key_f(e, "lightstrength").unwrap_or(1.0).max(0.0) as f32;
    c.map(|v| (v / 255.0).max(0.0) * strength * BRIGHTNESS)
}

#[cfg(test)]
pub(crate) fn describe(e: &Entity, aim: Option<DVec3>) -> Option<Projector> {
    describe_with(e, aim, None)
}

pub(crate) fn describe_with(
    e: &Entity,
    aim: Option<DVec3>,
    io: Option<&hp_app::state::ProjectorOverride>,
) -> Option<Projector> {
    if !e.classname.eq_ignore_ascii_case(PROJECTED_TEXTURE) {
        return None;
    }
    let flags = key_f(e, "spawnflags").unwrap_or(0.0) as i64;
    let on = io.and_then(|o| o.on).unwrap_or(flags & START_DISABLED == 0);
    let world = io.and_then(|o| o.light_world).unwrap_or(key_f(e, "lightworld") != Some(0.0));
    if !on || !world {
        return None;
    }
    let pos = e.origin().filter(|o| o.is_finite())?;
    let (forward, right, up) = match aim.and_then(|to| (to - pos).try_normalize()) {
        Some(dir) => basis_towards(dir),
        None => angle_vectors(e.angles().filter(|a| a.is_finite()).unwrap_or(DVec3::ZERO)),
    };
    let fov = io.and_then(|o| o.fov).or_else(|| key_f(e, "lightfov")).unwrap_or(DEFAULT_FOV).clamp(1.0, MAX_FOV) as f32;
    let near_z = key_f(e, "nearz").unwrap_or(4.0).max(0.01) as f32;
    let far_z = key_f(e, "farz").unwrap_or(750.0).max(0.1) as f32;
    Some(Projector {
        pos: pos.as_vec3().to_array(),
        forward: forward.as_vec3().to_array(),
        right: right.as_vec3().to_array(),
        up: up.as_vec3().to_array(),
        fov_deg: fov,
        near_z,
        far_z,
        color: light_color(e),
        attenuation: ATTENUATION,
        shadows: io.and_then(|o| o.shadows).unwrap_or_else(|| key_f(e, "enableshadows").is_none_or(|v| v != 0.0)),
        image: None,
    })
}

pub(crate) fn texture_name(e: &Entity) -> String {
    material_name(e.get("texturename").map(str::trim).filter(|s| !s.is_empty()).unwrap_or(DEFAULT_TEXTURE))
}

fn material_name(name: &str) -> String {
    let name = name.replace('\\', "/").to_ascii_lowercase();
    let name = name.strip_suffix(".vmt").or_else(|| name.strip_suffix(".vtf")).unwrap_or(&name).to_owned();
    name.trim_start_matches("materials/").to_owned()
}

fn cache() -> &'static Mutex<HashMap<String, ProjectorImage>> {
    static CACHE: OnceLock<Mutex<HashMap<String, ProjectorImage>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn load_image(fs: &hp_fs::GameFs, name: &str, frame: u32) -> Option<ProjectorImage> {
    let key = format!("{name}#{frame}");
    if let Some(hit) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return Some(hit);
    }
    let base = fs
        .read_text(&format!("materials/{name}.vmt"))
        .and_then(|t| hp_vmt::parse(&t).ok())
        .and_then(|m| m.base_texture())
        .unwrap_or_else(|| name.to_owned());
    let base = base.replace('\\', "/").to_ascii_lowercase();
    let bytes = fs.read(&format!("materials/{}.vtf", base.trim_start_matches("materials/")))?;
    let opts = hp_vtf::DecodeOptions { mip: hp_vtf::MipSelect::MaxDimension(256), frame, face: 0 };
    let img = hp_vtf::decode_rgba8(&bytes, &opts).ok()?;
    let out = ProjectorImage { width: img.width, height: img.height, rgba: img.pixels };
    if let Ok(mut c) = cache().lock() {
        c.insert(key, out.clone());
    }
    Some(out)
}

pub(crate) fn collect(
    doc: &Document,
    fs: Option<&hp_fs::GameFs>,
    targets: &AHashMap<String, DVec3>,
    overrides: &[hp_app::state::ProjectorOverride],
) -> Vec<Projector> {
    let mut nodes: Vec<_> = doc
        .scene()
        .entities()
        .iter()
        .filter(|(_, n)| n.entity.classname.eq_ignore_ascii_case(PROJECTED_TEXTURE))
        .map(|(id, n)| (*id, &n.entity))
        .collect();
    nodes.sort_unstable_by_key(|(id, _)| *id);
    let mut out = Vec::new();
    for (id, e) in nodes {
        if !doc.is_entity_visible(id) {
            continue;
        }
        let aim = e.get("target").and_then(|t| targets.get(t.trim())).copied();
        let io = overrides.iter().find(|o| o.id == id);
        let Some(mut p) = describe_with(e, aim, io) else { continue };
        let frame = key_f(e, "textureframe").unwrap_or(0.0).max(0.0) as u32;
        let name = io.and_then(|o| o.texture.as_deref()).map_or_else(|| texture_name(e), material_name);
        p.image = fs.and_then(|fs| load_image(fs, &name, frame));
        out.push(p);
        if out.len() >= hp_render::MAX_PROJECTORS {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(props: &[(&str, &str)]) -> Entity {
        let mut e = Entity { classname: PROJECTED_TEXTURE.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.set(k, *v);
        }
        e
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-5)
    }

    #[test]
    fn inputs_of_the_player_view_override_the_keys() {
        use hp_app::state::ProjectorOverride;
        let off = ent(&[("origin", "0 0 64"), ("spawnflags", "4"), ("lightfov", "30"), ("enableshadows", "1")]);
        assert!(describe(&off, None).is_none(), "Start Disabled");
        let on =
            ProjectorOverride { id: 1, on: Some(true), fov: Some(80.0), shadows: Some(false), ..Default::default() };
        let p = describe_with(&off, None, Some(&on)).expect("TurnOn");
        assert_eq!(p.fov_deg, 80.0);
        assert!(!p.shadows);
        let lit = ent(&[("origin", "0 0 64")]);
        assert!(
            describe_with(&lit, None, Some(&ProjectorOverride { on: Some(false), ..Default::default() })).is_none()
        );
        assert!(
            describe_with(&lit, None, Some(&ProjectorOverride { light_world: Some(false), ..Default::default() }))
                .is_none()
        );
        assert_eq!(material_name(r"Effects\Flashlight001.vmt"), "effects/flashlight001");
    }

    #[test]
    fn the_light_is_full_up_to_a_hundred_units_then_falls_with_the_distance() {
        assert_eq!(ATTENUATION, [0.0, 100.0, 0.0]);
        let att = |d: f32| (ATTENUATION[0] + (ATTENUATION[1] + ATTENUATION[2] / d) / d).clamp(0.0, 1.0);
        assert_eq!(att(20.0), 1.0);
        assert_eq!(att(100.0), 1.0);
        assert!((att(400.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn angles_follow_the_engine_basis() {
        let (f, r, u) = angle_vectors(DVec3::new(90.0, 0.0, 0.0));
        assert!(close(f.as_vec3().to_array(), [0.0, 0.0, -1.0]));
        assert!(close(r.as_vec3().to_array(), [0.0, -1.0, 0.0]));
        assert!(close(u.as_vec3().to_array(), [1.0, 0.0, 0.0]));
        let (f, r, u) = angle_vectors(DVec3::new(0.0, 90.0, 0.0));
        assert!(close(f.as_vec3().to_array(), [0.0, 1.0, 0.0]));
        assert!(close(r.as_vec3().to_array(), [1.0, 0.0, 0.0]));
        assert!(close(u.as_vec3().to_array(), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn aiming_matches_the_angles() {
        let (f, r, u) = basis_towards(DVec3::new(0.0, 0.0, -5.0));
        assert!(close(f.as_vec3().to_array(), [0.0, 0.0, -1.0]));
        assert!(f.dot(r).abs() < 1e-9 && f.dot(u).abs() < 1e-9 && r.dot(u).abs() < 1e-9);
        let (f, _, _) = basis_towards(DVec3::new(3.0, 3.0, 0.0));
        assert!(close(f.as_vec3().to_array(), [0.70710677, 0.70710677, 0.0]));
    }

    #[test]
    fn keys_set_the_frustum_and_color() {
        let e = ent(&[
            ("origin", "10 20 30"),
            ("angles", "90 0 0"),
            ("lightfov", "45"),
            ("nearz", "8"),
            ("farz", "300"),
            ("lightcolor", "255 128 0"),
            ("lightstrength", "2"),
        ]);
        let p = describe(&e, None).unwrap();
        assert_eq!((p.pos, p.fov_deg, p.near_z, p.far_z), ([10.0, 20.0, 30.0], 45.0, 8.0, 300.0));
        assert!(close(p.forward, [0.0, 0.0, -1.0]));
        assert!((p.color[0] - 2.0 * BRIGHTNESS).abs() < 1e-5 && p.color[2] == 0.0);
        let d = describe(&ent(&[("origin", "0 0 0")]), None).unwrap();
        assert_eq!((d.fov_deg, d.near_z, d.far_z), (45.0, 4.0, 750.0));
        assert!(close(d.color, [BRIGHTNESS; 3]));
    }

    #[test]
    fn disabled_and_non_world_projectors_are_left_out() {
        assert!(describe(&ent(&[("origin", "0 0 0"), ("spawnflags", "4")]), None).is_none());
        assert!(describe(&ent(&[("origin", "0 0 0"), ("lightworld", "0")]), None).is_none());
        let mut other = ent(&[("origin", "0 0 0")]);
        other.classname = "light".to_owned();
        assert!(describe(&other, None).is_none());
    }

    #[test]
    fn a_target_aims_the_projector() {
        let e = ent(&[("origin", "0 0 100"), ("angles", "0 0 0")]);
        let p = describe(&e, Some(DVec3::new(0.0, 0.0, 0.0))).unwrap();
        assert!(close(p.forward, [0.0, 0.0, -1.0]));
    }

    #[test]
    fn texture_names_are_normalized() {
        assert_eq!(texture_name(&ent(&[])), "effects/flashlight001");
        assert_eq!(texture_name(&ent(&[("texturename", "Materials\\Effects\\Spot.VMT")])), "effects/spot");
    }
}
