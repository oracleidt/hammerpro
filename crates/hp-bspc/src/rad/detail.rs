use glam::Vec3A;
use rayon::prelude::*;

use super::bake::{Ctx, FaceLight, Part, Sample, SurfHit};
use super::lights::Kind;
use super::output::{encode, finish, median};
use crate::detail_props::{self, DetailLump, kind};
use crate::props::ModelSource;

const STYLES: usize = 256;

pub(crate) const ANORMS: [[f32; 3]; 162] = [
    [-0.525731, 0.000000, 0.850651],
    [-0.442863, 0.238856, 0.864188],
    [-0.295242, 0.000000, 0.955423],
    [-0.309017, 0.500000, 0.809017],
    [-0.162460, 0.262866, 0.951056],
    [0.000000, 0.000000, 1.000000],
    [0.000000, 0.850651, 0.525731],
    [-0.147621, 0.716567, 0.681718],
    [0.147621, 0.716567, 0.681718],
    [0.000000, 0.525731, 0.850651],
    [0.309017, 0.500000, 0.809017],
    [0.525731, 0.000000, 0.850651],
    [0.295242, 0.000000, 0.955423],
    [0.442863, 0.238856, 0.864188],
    [0.162460, 0.262866, 0.951056],
    [-0.681718, 0.147621, 0.716567],
    [-0.809017, 0.309017, 0.500000],
    [-0.587785, 0.425325, 0.688191],
    [-0.850651, 0.525731, 0.000000],
    [-0.864188, 0.442863, 0.238856],
    [-0.716567, 0.681718, 0.147621],
    [-0.688191, 0.587785, 0.425325],
    [-0.500000, 0.809017, 0.309017],
    [-0.238856, 0.864188, 0.442863],
    [-0.425325, 0.688191, 0.587785],
    [-0.716567, 0.681718, -0.147621],
    [-0.500000, 0.809017, -0.309017],
    [-0.525731, 0.850651, 0.000000],
    [0.000000, 0.850651, -0.525731],
    [-0.238856, 0.864188, -0.442863],
    [0.000000, 0.955423, -0.295242],
    [-0.262866, 0.951056, -0.162460],
    [0.000000, 1.000000, 0.000000],
    [0.000000, 0.955423, 0.295242],
    [-0.262866, 0.951056, 0.162460],
    [0.238856, 0.864188, 0.442863],
    [0.262866, 0.951056, 0.162460],
    [0.500000, 0.809017, 0.309017],
    [0.238856, 0.864188, -0.442863],
    [0.262866, 0.951056, -0.162460],
    [0.500000, 0.809017, -0.309017],
    [0.850651, 0.525731, 0.000000],
    [0.716567, 0.681718, 0.147621],
    [0.716567, 0.681718, -0.147621],
    [0.525731, 0.850651, 0.000000],
    [0.425325, 0.688191, 0.587785],
    [0.864188, 0.442863, 0.238856],
    [0.688191, 0.587785, 0.425325],
    [0.809017, 0.309017, 0.500000],
    [0.681718, 0.147621, 0.716567],
    [0.587785, 0.425325, 0.688191],
    [0.955423, 0.295242, 0.000000],
    [1.000000, 0.000000, 0.000000],
    [0.951056, 0.162460, 0.262866],
    [0.850651, -0.525731, 0.000000],
    [0.955423, -0.295242, 0.000000],
    [0.864188, -0.442863, 0.238856],
    [0.951056, -0.162460, 0.262866],
    [0.809017, -0.309017, 0.500000],
    [0.681718, -0.147621, 0.716567],
    [0.850651, 0.000000, 0.525731],
    [0.864188, 0.442863, -0.238856],
    [0.809017, 0.309017, -0.500000],
    [0.951056, 0.162460, -0.262866],
    [0.525731, 0.000000, -0.850651],
    [0.681718, 0.147621, -0.716567],
    [0.681718, -0.147621, -0.716567],
    [0.850651, 0.000000, -0.525731],
    [0.809017, -0.309017, -0.500000],
    [0.864188, -0.442863, -0.238856],
    [0.951056, -0.162460, -0.262866],
    [0.147621, 0.716567, -0.681718],
    [0.309017, 0.500000, -0.809017],
    [0.425325, 0.688191, -0.587785],
    [0.442863, 0.238856, -0.864188],
    [0.587785, 0.425325, -0.688191],
    [0.688191, 0.587785, -0.425325],
    [-0.147621, 0.716567, -0.681718],
    [-0.309017, 0.500000, -0.809017],
    [0.000000, 0.525731, -0.850651],
    [-0.525731, 0.000000, -0.850651],
    [-0.442863, 0.238856, -0.864188],
    [-0.295242, 0.000000, -0.955423],
    [-0.162460, 0.262866, -0.951056],
    [0.000000, 0.000000, -1.000000],
    [0.295242, 0.000000, -0.955423],
    [0.162460, 0.262866, -0.951056],
    [-0.442863, -0.238856, -0.864188],
    [-0.309017, -0.500000, -0.809017],
    [-0.162460, -0.262866, -0.951056],
    [0.000000, -0.850651, -0.525731],
    [-0.147621, -0.716567, -0.681718],
    [0.147621, -0.716567, -0.681718],
    [0.000000, -0.525731, -0.850651],
    [0.309017, -0.500000, -0.809017],
    [0.442863, -0.238856, -0.864188],
    [0.162460, -0.262866, -0.951056],
    [0.238856, -0.864188, -0.442863],
    [0.500000, -0.809017, -0.309017],
    [0.425325, -0.688191, -0.587785],
    [0.716567, -0.681718, -0.147621],
    [0.688191, -0.587785, -0.425325],
    [0.587785, -0.425325, -0.688191],
    [0.000000, -0.955423, -0.295242],
    [0.000000, -1.000000, 0.000000],
    [0.262866, -0.951056, -0.162460],
    [0.000000, -0.850651, 0.525731],
    [0.000000, -0.955423, 0.295242],
    [0.238856, -0.864188, 0.442863],
    [0.262866, -0.951056, 0.162460],
    [0.500000, -0.809017, 0.309017],
    [0.716567, -0.681718, 0.147621],
    [0.525731, -0.850651, 0.000000],
    [-0.238856, -0.864188, -0.442863],
    [-0.500000, -0.809017, -0.309017],
    [-0.262866, -0.951056, -0.162460],
    [-0.850651, -0.525731, 0.000000],
    [-0.716567, -0.681718, -0.147621],
    [-0.716567, -0.681718, 0.147621],
    [-0.525731, -0.850651, 0.000000],
    [-0.500000, -0.809017, 0.309017],
    [-0.238856, -0.864188, 0.442863],
    [-0.262866, -0.951056, 0.162460],
    [-0.864188, -0.442863, 0.238856],
    [-0.809017, -0.309017, 0.500000],
    [-0.688191, -0.587785, 0.425325],
    [-0.681718, -0.147621, 0.716567],
    [-0.442863, -0.238856, 0.864188],
    [-0.587785, -0.425325, 0.688191],
    [-0.309017, -0.500000, 0.809017],
    [-0.147621, -0.716567, 0.681718],
    [-0.425325, -0.688191, 0.587785],
    [-0.162460, -0.262866, 0.951056],
    [0.442863, -0.238856, 0.864188],
    [0.162460, -0.262866, 0.951056],
    [0.309017, -0.500000, 0.809017],
    [0.147621, -0.716567, 0.681718],
    [0.000000, -0.525731, 0.850651],
    [0.425325, -0.688191, 0.587785],
    [0.587785, -0.425325, 0.688191],
    [0.688191, -0.587785, 0.425325],
    [-0.955423, 0.295242, 0.000000],
    [-0.951056, 0.162460, 0.262866],
    [-1.000000, 0.000000, 0.000000],
    [-0.850651, 0.000000, 0.525731],
    [-0.955423, -0.295242, 0.000000],
    [-0.951056, -0.162460, 0.262866],
    [-0.864188, 0.442863, -0.238856],
    [-0.951056, 0.162460, -0.262866],
    [-0.809017, 0.309017, -0.500000],
    [-0.864188, -0.442863, -0.238856],
    [-0.951056, -0.162460, -0.262866],
    [-0.809017, -0.309017, -0.500000],
    [-0.681718, 0.147621, -0.716567],
    [-0.681718, -0.147621, -0.716567],
    [-0.850651, 0.000000, -0.525731],
    [-0.688191, 0.587785, -0.425325],
    [-0.587785, 0.425325, -0.688191],
    [-0.425325, 0.688191, -0.587785],
    [-0.425325, -0.688191, -0.587785],
    [-0.587785, -0.425325, -0.688191],
    [-0.688191, -0.587785, -0.425325],
];

fn angle_vectors(a: glam::Vec3) -> (Vec3A, Vec3A, Vec3A) {
    let (sp, cp) = a.x.to_radians().sin_cos();
    let (sy, cy) = a.y.to_radians().sin_cos();
    let (sr, cr) = a.z.to_radians().sin_cos();
    let forward = Vec3A::new(cp * cy, cp * sy, -sp);
    let right = Vec3A::new(-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp);
    let up = Vec3A::new(cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp);
    (forward, right, up)
}

#[derive(Clone, Debug, Default)]
struct ObjectLight {
    base: Vec3A,
    styles: Vec<(u8, Vec3A)>,
}

pub(crate) fn count(src: &hp_bsp::Bsp) -> usize {
    let Ok(games) = src.game_lumps() else { return 0 };
    let Some(g) = games.get(detail_props::DPRP) else { return 0 };
    if g.version != detail_props::DPRP_VERSION {
        return 0;
    }
    g.data().ok().and_then(|d| DetailLump::parse(&d).ok()).map_or(0, |l| l.objects.len())
}

pub(crate) fn light(
    src: &hp_bsp::Bsp,
    ctx: &Ctx<'_>,
    light: &[FaceLight],
    models: &dyn ModelSource,
    passes: &[bool],
) -> Result<Option<(hp_bsp::GameLumps, usize)>, String> {
    let fail = |e: hp_bsp::BspError| e.to_string();
    let Ok(mut games) = src.game_lumps() else { return Ok(None) };
    let Some(g) = games.get(detail_props::DPRP) else { return Ok(None) };
    if g.version != detail_props::DPRP_VERSION {
        return Ok(None);
    }
    let mut lump = DetailLump::parse(&g.data().map_err(fail)?)?;
    if lump.objects.is_empty() {
        return Ok(None);
    }
    let model_centers: Vec<Vec3A> =
        lump.models.iter().map(|m| models.hull_center(m).map_or(Vec3A::ZERO, |c| c.as_vec3a())).collect();
    let sprite_centers: Vec<Vec3A> =
        lump.sprites.iter().map(|s| Vec3A::new(0.0, s[2] + s[0], s[3] + s[1]) * 0.5).collect();
    let averages: Vec<Vec<Vec<Vec3A>>> = ctx
        .faces
        .par_iter()
        .zip(light)
        .map(|(f, l)| {
            let lux = f.luxels();
            (0..passes.len())
                .map(|p| {
                    (0..l.styles.len())
                        .map(|slot| {
                            let at = l.index(p, slot, 0, lux);
                            let v: Vec<Vec3A> = l.values[at..at + lux].iter().map(|v| finish(*v, f.minlight)).collect();
                            median(&v)
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    let cand: Vec<(usize, usize)> = ctx
        .lights
        .list
        .iter()
        .enumerate()
        .filter(|(_, l)| l.kind != Kind::SkyAmbient && usize::from(l.style) < STYLES)
        .map(|(i, l)| (i, usize::from(l.style)))
        .collect();
    let np = passes.len();
    let sky: Vec<Vec3A> = (0..np)
        .map(|p| {
            ctx.lights
                .list
                .iter()
                .find(|l| l.kind == Kind::SkyAmbient)
                .and_then(|l| l.intensity.get(p))
                .copied()
                .unwrap_or(Vec3A::ZERO)
        })
        .collect();
    let tan_theta = 7.275f32.to_radians().tan();
    ctx.work.begin(super::progress::Stage::DetailProps, lump.objects.len() as u64);
    let placed: Vec<Option<(Vec3A, Vec3A, i32)>> = lump
        .objects
        .par_iter()
        .map(|o| {
            let (forward, right, up) = angle_vectors(o.angles);
            let offset = match o.kind {
                kind::MODEL => model_centers.get(usize::from(o.dict)).copied().unwrap_or(Vec3A::ZERO),
                kind::SPRITE => sprite_centers.get(usize::from(o.dict)).copied().unwrap_or(Vec3A::ZERO) * o.scale,
                _ => Vec3A::ZERO,
            };
            let center = Vec3A::from(o.origin) + forward * offset.x - right * offset.y + up * offset.z;
            (center.is_finite() && up.is_finite()).then(|| (center, up, ctx.map.cluster_at(center.as_dvec3())))
        })
        .collect();
    let mut clusters: Vec<i32> = placed.iter().flatten().map(|p| p.2).collect();
    clusters.sort_unstable();
    clusters.dedup();
    let seen_by: std::collections::HashMap<i32, Vec<(usize, usize)>> = clusters
        .par_iter()
        .map(|&c| {
            let list = cand
                .iter()
                .copied()
                .filter(|&(li, _)| super::bake::row_sees(ctx.light_rows.get(li).and_then(Option::as_ref), c))
                .collect();
            (c, list)
        })
        .collect();
    let lit: Vec<Vec<ObjectLight>> = placed
        .par_iter()
        .map_init(
            || (vec![Vec3A::ZERO; np * STYLES], vec![Vec3A::ZERO; np * STYLES]),
            |(direct, ambient), placed| {
                let Some((center, up, cluster)) = *placed else {
                    return vec![ObjectLight { base: Vec3A::new(2.0, 0.0, 0.0), styles: Vec::new() }; np];
                };
                let sample = Sample { pos: center, normals: [up; 4], cluster };
                direct.fill(Vec3A::ZERO);
                ambient.fill(Vec3A::ZERO);
                let near = seen_by.get(&cluster).map_or(&cand[..], Vec::as_slice);
                ctx.gather(near, STYLES, 1, &sample, Part::Direct, direct);
                for d in &ANORMS {
                    match ctx.trace_surface(center, Vec3A::from_array(*d)) {
                        SurfHit::Sky => {
                            for p in 0..np {
                                ambient[p * STYLES] += sky[p] / 255.0;
                            }
                        }
                        SurfHit::Face(fi, s, t, dist) => {
                            let (Some(f), Some(l)) = (ctx.faces.get(fi as usize), light.get(fi as usize)) else {
                                continue;
                            };
                            let scale_avg = ((dist * tan_theta - 20.0) / 20.0).clamp(0.0, 1.0);
                            let lux = f.luxels();
                            let (x, y) = (
                                (s.round().max(0.0) as usize).min(f.w - 1),
                                (t.round().max(0.0) as usize).min(f.h - 1),
                            );
                            for p in 0..np {
                                for (slot, style) in l.styles.iter().enumerate() {
                                    let style = usize::from(*style);
                                    if style >= STYLES {
                                        continue;
                                    }
                                    let avg = averages[fi as usize][p][slot];
                                    let point = finish(l.values[l.index(p, slot, 0, lux) + y * f.w + x], f.minlight);
                                    ambient[p * STYLES + style] +=
                                        (avg * scale_avg + point * (1.0 - scale_avg)) / 255.0 * f.reflectivity;
                                }
                            }
                        }
                        SurfHit::None => {}
                    }
                }
                (0..np)
                    .map(|p| {
                        let total =
                            |s: usize| direct[p * STYLES + s] + ambient[p * STYLES + s] * (255.0 / ANORMS.len() as f32);
                        let styles = (1..STYLES)
                            .filter_map(|s| {
                                let c = total(s) * 0.5;
                                (c != Vec3A::ZERO).then_some((s as u8, c))
                            })
                            .collect();
                        ObjectLight { base: total(0), styles }
                    })
                    .collect()
            },
        )
        .inspect(|_| ctx.work.add(1))
        .collect();
    for (p, &hdr) in passes.iter().enumerate() {
        let mut records: Vec<([u8; 4], u8)> = Vec::new();
        for (o, l) in lump.objects.iter_mut().zip(&lit) {
            let l = &l[p];
            o.lighting = encode(l.base);
            o.light_styles = if l.styles.is_empty() { 0 } else { records.len() as u32 };
            o.light_style_count = l.styles.len().min(255) as u8;
            records.extend(l.styles.iter().take(255).map(|(s, c)| (encode(*c), *s)));
        }
        let id = if hdr { detail_props::DPLH } else { detail_props::DPLT };
        games.set(id, detail_props::DPL_VERSION, detail_props::styles_to_bytes(&records)).map_err(fail)?;
    }
    for id in [detail_props::DPLT, detail_props::DPLH] {
        if games.get(id).is_none() {
            games.set(id, detail_props::DPL_VERSION, detail_props::styles_to_bytes(&[])).map_err(fail)?;
        }
    }
    let count = lump.objects.len();
    games.set(detail_props::DPRP, detail_props::DPRP_VERSION, lump.to_bytes()).map_err(fail)?;
    Ok(Some((games, count)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vectors_of_angles() {
        let (f, r, u) = angle_vectors(glam::Vec3::ZERO);
        assert!((f - Vec3A::X).length() < 1e-6 && (r + Vec3A::Y).length() < 1e-6 && (u - Vec3A::Z).length() < 1e-6);
        let (f, _, u) = angle_vectors(glam::Vec3::new(0.0, 90.0, 0.0));
        assert!((f - Vec3A::Y).length() < 1e-6 && (u - Vec3A::Z).length() < 1e-6);
        assert!(ANORMS.iter().all(|n| (Vec3A::from_array(*n).length() - 1.0).abs() < 1e-4));
    }
}
