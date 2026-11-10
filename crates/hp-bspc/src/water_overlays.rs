use ahash::AHashMap;
use glam::DVec3;

use crate::compile::CompileError;
use crate::emit::Buf;
use crate::materials::MaterialSource;
use crate::texinfo::{TexInfo, TexInfos};

pub const CLASS: &str = "info_overlay_transition";
pub const RECORD_SIZE: usize = 1120;
pub const MAX_FACES: usize = 256;
const TRANSITION_BLOCK: &str = "overlaytransition";
const DATA_BLOCK: &str = "overlaydata";

#[derive(Clone, Debug, PartialEq)]
pub struct WaterOverlay {
    pub material: String,
    pub sides: Vec<u32>,
    pub u: [f64; 2],
    pub v: [f64; 2],
    pub origin: DVec3,
    pub basis_u: DVec3,
    pub basis_v: DVec3,
    pub normal: DVec3,
    pub uv: [DVec3; 4],
}

fn num(s: Option<&str>, default: f64) -> f64 {
    s.and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(default)
}

fn vec3(s: Option<&str>) -> DVec3 {
    let v: Vec<f64> = s.unwrap_or("").split_whitespace().filter_map(|x| x.parse().ok()).collect();
    if v.len() >= 3 && v.iter().take(3).all(|x| x.is_finite()) { DVec3::new(v[0], v[1], v[2]) } else { DVec3::ZERO }
}

fn side_list(s: Option<&str>) -> Vec<u32> {
    s.unwrap_or("").split_whitespace().filter_map(|x| x.parse().ok()).collect()
}

fn from_block(e: &hp_vmf::Entity, b: &hp_kv::Block) -> WaterOverlay {
    let material = b.get("material").or_else(|| e.get("material")).unwrap_or("").trim().to_owned();
    WaterOverlay {
        material,
        sides: side_list(b.get("sides")),
        u: [num(b.get("StartU"), 0.0), num(b.get("EndU"), 1.0)],
        v: [num(b.get("StartV"), 0.0), num(b.get("EndV"), 1.0)],
        origin: vec3(b.get("BasisOrigin")),
        basis_u: vec3(b.get("BasisU")),
        basis_v: vec3(b.get("BasisV")),
        normal: vec3(b.get("BasisNormal")),
        uv: [vec3(b.get("uv0")), vec3(b.get("uv1")), vec3(b.get("uv2")), vec3(b.get("uv3"))],
    }
}

pub fn side_polygons(entities: &[&hp_vmf::Entity], wanted: &[u32]) -> AHashMap<u32, (Vec<DVec3>, DVec3)> {
    let mut out = AHashMap::new();
    if wanted.is_empty() {
        return out;
    }
    for e in entities {
        for solid in &e.solids {
            if !solid.sides.iter().any(|s| wanted.contains(&s.id)) {
                continue;
            }
            let geom = hp_geom::build_solid(&solid.sides);
            for (k, side) in solid.sides.iter().enumerate() {
                if !wanted.contains(&side.id) {
                    continue;
                }
                if let Some(f) = geom.face_for_side(k).filter(|f| f.vertices.len() >= 3) {
                    out.insert(side.id, (f.vertices.clone(), f.normal()));
                }
            }
        }
    }
    out
}

pub fn from_shoreline(e: &hp_vmf::Entity, polys: &AHashMap<u32, (Vec<DVec3>, DVec3)>) -> Vec<WaterOverlay> {
    let land = side_list(e.get("sides"));
    let water = side_list(e.get("sides2"));
    let width_land = num(e.get("Width1"), 25.0).clamp(0.0, 4096.0);
    let width_water = num(e.get("Width2"), 25.0).clamp(0.0, 4096.0);
    let length_tex = [num(e.get("LengthTexcoordStart"), 0.0), num(e.get("LengthTexcoordEnd"), 1.0)];
    let width_tex = [num(e.get("WidthTexcoordStart"), 0.0), num(e.get("WidthTexcoordEnd"), 1.0)];
    let material = e.get("material").unwrap_or("").trim().to_owned();
    let mut out = Vec::new();
    let lands: Vec<(&[DVec3], DVec3)> =
        land.iter().filter_map(|ls| polys.get(ls)).map(|(poly, n)| (poly.as_slice(), *n)).collect();
    for &ws in &water {
        let Some((wpoly, wn)) = polys.get(&ws) else { continue };
        let wd = wn.dot(wpoly[0]);
        let strips = hp_geom::shoreline::shore_strips(&lands, *wn, wd);
        let total: f64 = strips.iter().map(hp_geom::shoreline::ShoreStrip::length).sum();
        if total <= 1e-6 {
            continue;
        }
        let tex = |l: f64| length_tex[0] + (length_tex[1] - length_tex[0]) * (l / total);
        for strip in strips {
            let len = strip.length();
            let basis_u = (strip.end - strip.start) / len;
            let half = len * 0.5;
            out.push(WaterOverlay {
                material: material.clone(),
                sides: vec![ws],
                u: [tex(strip.along), tex(strip.along + len)],
                v: width_tex,
                origin: (strip.start + strip.end) * 0.5,
                basis_u,
                basis_v: wn.cross(basis_u),
                normal: *wn,
                uv: [
                    DVec3::new(-half, -width_land, 0.0),
                    DVec3::new(-half, width_water, 0.0),
                    DVec3::new(half, width_water, 0.0),
                    DVec3::new(half, -width_land, 0.0),
                ],
            });
        }
    }
    out
}

fn upright(mut o: WaterOverlay) -> WaterOverlay {
    if o.normal.cross(o.basis_u).dot(o.basis_v) >= 0.0 {
        return o;
    }
    let mirror = |p: DVec3| DVec3::new(-p.x, p.y, p.z);
    o.basis_u = -o.basis_u;
    o.uv = [mirror(o.uv[3]), mirror(o.uv[2]), mirror(o.uv[1]), mirror(o.uv[0])];
    o.u = [o.u[1], o.u[0]];
    o
}

pub fn overlays_of(entities: &[&hp_vmf::Entity]) -> Vec<(DVec3, Vec<WaterOverlay>)> {
    let transitions: Vec<&&hp_vmf::Entity> =
        entities.iter().filter(|e| e.classname.trim().eq_ignore_ascii_case(CLASS)).collect();
    if transitions.is_empty() {
        return Vec::new();
    }
    let wanted: Vec<u32> = transitions
        .iter()
        .flat_map(|e| side_list(e.get("sides")).into_iter().chain(side_list(e.get("sides2"))))
        .collect();
    let polys = side_polygons(entities, &wanted);
    transitions
        .iter()
        .map(|e| {
            let origin = vec3(e.get("origin"));
            let computed = from_shoreline(e, &polys);
            if !computed.is_empty() {
                return (origin, computed);
            }
            let saved = e
                .extra_blocks
                .iter()
                .filter(|b| b.name.eq_ignore_ascii_case(TRANSITION_BLOCK))
                .flat_map(|b| b.children.iter().filter(|c| c.name.eq_ignore_ascii_case(DATA_BLOCK)))
                .map(|b| from_block(e, b))
                .collect();
            (origin, saved)
        })
        .collect()
}

pub fn lump(
    entities: &[&hp_vmf::Entity],
    side_faces: &AHashMap<u32, Vec<u32>>,
    tex: &mut TexInfos,
    materials: &dyn MaterialSource,
    first_id: usize,
    max_overlays: usize,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<u8>, CompileError> {
    let mut out = Buf::default();
    let mut count = 0usize;
    for (origin, list) in overlays_of(entities) {
        if list.is_empty() {
            log(&format!(
                "Warning: info_overlay_transition at ({:.0} {:.0} {:.0}) makes no water overlay: its shore faces \
                 (sides) do not cross the plane of its water faces (sides2)",
                origin.x, origin.y, origin.z
            ));
            continue;
        }
        for o in list.into_iter().map(upright) {
            if o.basis_u.length_squared() < 1e-12 || o.normal.length_squared() < 1e-12 {
                log(&format!(
                    "Warning: water overlay of the info_overlay_transition at ({:.0} {:.0} {:.0}) skipped: its saved \
                     strip has no basis",
                    origin.x, origin.y, origin.z
                ));
                continue;
            }
            if count >= max_overlays {
                return Err(CompileError::Failed("Too many water overlays!".to_owned()));
            }
            let mut faces: Vec<u32> = o.sides.iter().filter_map(|s| side_faces.get(s)).flatten().copied().collect();
            faces.sort_unstable();
            faces.dedup();
            if faces.len() > MAX_FACES {
                return Err(CompileError::Failed(format!(
                    "Water Overlay touching too many faces (touching {}, max {MAX_FACES})\nOverlay {} at {:.1} {:.1} {:.1}",
                    faces.len(),
                    o.material,
                    o.origin.x,
                    o.origin.y,
                    o.origin.z
                )));
            }
            if faces.is_empty() {
                log(&format!(
                    "Warning: water overlay on no face (info_overlay_transition at {:.0} {:.0} {:.0}): its water sides are gone",
                    origin.x, origin.y, origin.z
                ));
            }
            let facts = materials.facts(&o.material);
            let texdata = tex.texdata(&o.material, &facts);
            let none = [0.0, 0.0, 0.0, -99999.0];
            let texinfo = tex.add(TexInfo { texture: [none; 2], lightmap: [none; 2], flags: 0, texdata });
            out.i32((first_id + count) as i32);
            out.i16(texinfo as i16);
            out.u16(faces.len() as u16);
            for k in 0..MAX_FACES {
                out.i32(faces.get(k).map_or(0, |f| *f as i32));
            }
            out.f32(o.u[0] as f32);
            out.f32(o.u[1] as f32);
            out.f32(o.v[0] as f32);
            out.f32(o.v[1] as f32);
            let zs = [o.basis_u.x, o.basis_u.y, o.basis_u.z, 0.0];
            for (uv, z) in o.uv.iter().zip(zs) {
                out.f32(uv.x as f32);
                out.f32(uv.y as f32);
                out.f32(z as f32);
            }
            out.vec3(o.origin);
            out.vec3(o.normal);
            count += 1;
        }
    }
    Ok(out.0)
}
