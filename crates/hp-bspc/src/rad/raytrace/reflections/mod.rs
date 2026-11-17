mod render;
mod scene;
mod sky;
#[cfg(test)]
mod tests;
mod vtf;

use glam::Vec3A;
use rayon::prelude::*;

use super::{Quality, RayTracing};
use crate::props::ModelSource;

type Faces = [[Vec<Vec3A>; 6]; 2];

pub(crate) const DEFAULT_SIZE: u32 = 32;
const MAX_SIZE: u32 = 1024;
const MIN_SIZE: u32 = 4;
const PAK_BUDGET: u64 = 96 << 20;
const RAYS_FINAL: u32 = 1 << 17;
const RAYS_NORMAL: u32 = 1 << 15;

pub(crate) fn wanted(rt: &RayTracing) -> bool {
    rt.effects.reflections.unwrap_or(matches!(rt.quality, Quality::Final | Quality::Optimal))
}

pub(crate) fn pixels(size: i32) -> u32 {
    if size <= 0 { DEFAULT_SIZE } else { (1u32 << (size - 1).min(15) as u32).clamp(MIN_SIZE, MAX_SIZE) }
}

fn samples_for(size: u32, quality: Quality) -> u32 {
    let budget = if matches!(quality, Quality::Final | Quality::Optimal) { RAYS_FINAL } else { RAYS_NORMAL };
    (budget / (size * size).max(1)).clamp(4, 256)
}

pub(crate) fn texture_name(map: &str, origin: [i32; 3]) -> String {
    format!("materials/maps/{map}/c{}_{}_{}", origin[0], origin[1], origin[2])
}

pub(crate) fn map_name(src: &hp_bsp::Bsp) -> Option<String> {
    let names: Vec<String> = src
        .pakfile()
        .ok()
        .map(|p| p.entries().iter().map(|e| e.name.to_ascii_lowercase()).collect())
        .unwrap_or_default();
    let folder = |n: &str| {
        n.strip_prefix("materials/maps/")
            .and_then(|r| r.split_once('/'))
            .map(|(m, rest)| (m.to_owned(), rest.to_owned()))
    };
    let pick = |want: &dyn Fn(&str) -> bool| {
        names.iter().filter_map(|n| folder(n)).find(|(_, rest)| want(rest)).map(|(m, _)| m)
    };
    pick(&|rest| rest.starts_with("cubemapdefault."))
        .or_else(|| pick(&|rest| rest.starts_with('c') && rest.ends_with(".vtf") && !rest.contains('/')))
        .or_else(|| pick(&|rest| rest.ends_with(".vmt")))
        .or_else(|| {
            src.texture_names().ok()?.iter().find_map(|t| {
                let t = t.replace('\\', "/").to_ascii_lowercase();
                t.strip_prefix("maps/")?.split_once('/').map(|(m, _)| m.to_owned())
            })
        })
}

pub(crate) struct Job {
    pub name: String,
    pub origin: Option<[i32; 3]>,
    pub size: u32,
}

fn bytes_for(size: u32, hdr: bool) -> u64 {
    let mut total = 0u64;
    for m in 0..vtf::mip_count(size) {
        total += vtf::level_bytes((size >> m).max(1), hdr) as u64 * vtf::FACES as u64;
    }
    total
}

pub(crate) fn plan(map: &str, cubes: &[([i32; 3], i32)], passes: [bool; 2]) -> (Vec<Job>, bool) {
    let mut jobs: Vec<Job> = Vec::new();
    for (origin, size) in cubes {
        let name = texture_name(map, *origin);
        if jobs.iter().any(|j| j.name == name) {
            continue;
        }
        jobs.push(Job { name, origin: Some(*origin), size: pixels(*size) });
    }
    jobs.push(Job { name: format!("materials/maps/{map}/cubemapdefault"), origin: None, size: DEFAULT_SIZE });
    let cost = |jobs: &[Job]| -> u64 {
        jobs.iter().map(|j| (0..2).filter(|p| passes[*p]).map(|p| bytes_for(j.size, p == 1)).sum::<u64>()).sum()
    };
    let mut shrunk = false;
    while cost(&jobs) > PAK_BUDGET {
        let Some(big) = jobs.iter_mut().filter(|j| j.size > DEFAULT_SIZE).max_by_key(|j| j.size) else { break };
        big.size /= 2;
        shrunk = true;
    }
    (jobs, shrunk)
}

pub(crate) fn bake(
    bytes: Vec<u8>,
    models: &dyn ModelSource,
    rt: &RayTracing,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<u8>, String> {
    let t = std::time::Instant::now();
    let src = hp_bsp::Bsp::from_vec(bytes).map_err(|e| format!("not a BSP file: {e}"))?;
    let cubes = src.cubemaps().unwrap_or_default();
    let Some(map) = map_name(&src) else {
        log(
            "ray traced reflections: the map name is not known (no materials/maps folder in the pakfile), cubemaps not rendered",
        );
        return Ok(src.into_vec());
    };
    let with_pak = crate::rad::props::MapPak { pak: src.pakfile().ok().filter(|p| !p.is_empty()), inner: models };
    let scene = scene::build(&src, &with_pak)?;
    drop(with_pak);
    for l in &scene.lines {
        log(l);
    }
    if !scene.passes.iter().any(|p| *p) {
        log("ray traced reflections: the map has no lighting, cubemaps not rendered");
        return Ok(src.into_vec());
    }
    let (jobs, shrunk) = plan(&map, &cubes, scene.passes);
    if shrunk {
        log(&format!(
            "ray traced reflections: the largest cubemaps are rendered at a smaller size so they fit in {} MB of the pakfile",
            PAK_BUDGET >> 20
        ));
    }
    let seed = rt.seed;
    let rendered: Vec<(String, Faces, u32)> = jobs
        .iter()
        .enumerate()
        .map(|(i, j)| {
            let faces = match j.origin {
                Some(o) => scene.render(
                    Vec3A::new(o[0] as f32, o[1] as f32, o[2] as f32),
                    j.size,
                    samples_for(j.size, rt.quality),
                    seed.wrapping_add(i as u32),
                ),
                None => scene.render_sky(j.size),
            };
            (j.name.clone(), faces, j.size)
        })
        .collect();
    let files: Vec<(String, Vec<u8>)> = rendered
        .par_iter()
        .flat_map_iter(|(name, faces, size)| {
            (0..2)
                .filter(|p| scene.passes[*p])
                .map(|p| {
                    let ext = if p == 1 { "hdr.vtf" } else { "vtf" };
                    (format!("{name}.{ext}"), vtf::cubemap(&faces[p], *size, p == 1))
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let added: usize = files.iter().map(|(_, d)| d.len()).sum();
    let count = files.len();
    let pak = crate::rad::props::add_to_pak(&src, files)?;
    let mut edit = src.edit();
    edit.set_pakfile(pak).map_err(|e| e.to_string())?;
    let out = edit.to_vec().map_err(|e| e.to_string())?;
    let mut sizes: Vec<u32> = jobs.iter().filter(|j| j.origin.is_some()).map(|j| j.size).collect();
    sizes.sort_unstable();
    sizes.dedup();
    log(&format!(
        "ray traced reflections: {} env_cubemap rendered ({} px) and the default cubemap, {} {} textures ({} KB) written to materials/maps/{map}/ in the pakfile in {:.1} s, no buildcubemaps needed",
        jobs.len() - 1,
        sizes.iter().map(u32::to_string).collect::<Vec<_>>().join(", "),
        count,
        match scene.passes {
            [true, true] => "LDR and HDR",
            [false, true] => "HDR",
            _ => "LDR",
        },
        added / 1024,
        t.elapsed().as_secs_f32()
    ));
    Ok(out)
}
