mod render;
mod scene;
mod sky;
#[cfg(test)]
mod tests;
mod vtf;

use std::time::{Duration, Instant};

use glam::Vec3A;
use rayon::prelude::*;

use render::{Limits, Outcome};

use crate::props::ModelSource;

type Faces = [[Vec<Vec3A>; 6]; 2];

pub(crate) const DEFAULT_SIZE: u32 = 32;
const MAX_SIZE: u32 = 1024;
const MIN_SIZE: u32 = 4;
const PAK_BUDGET: u64 = 96 << 20;
const RAYS_FINAL: u32 = 1 << 17;
const RAYS_NORMAL: u32 = 1 << 15;
const CUBE_SOFT: Duration = Duration::from_secs(15);
const CUBE_HARD: Duration = Duration::from_secs(90);
const TOTAL_SOFT: Duration = Duration::from_secs(20 * 60);
const TOTAL_HARD: Duration = Duration::from_secs(40 * 60);
const PROGRESS_EVERY: Duration = Duration::from_secs(10);
const DETAIL_LINES: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Budget {
    pub cube_soft: Duration,
    pub cube_hard: Duration,
    pub total_soft: Duration,
    pub total_hard: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Budget { cube_soft: CUBE_SOFT, cube_hard: CUBE_HARD, total_soft: TOTAL_SOFT, total_hard: TOTAL_HARD }
    }
}

impl Budget {
    pub(crate) fn limits(&self, size: u32, samples: u32, elapsed: Duration) -> Option<Limits> {
        let left = self.total_hard.checked_sub(elapsed).filter(|d| !d.is_zero())?;
        let grid = (samples.max(1) as f32).sqrt().ceil() as u64;
        let rays = 6 * u64::from(size) * u64::from(size) * grid * grid;
        let scale = (rays as f64 / (6.0 * f64::from(RAYS_FINAL))).max(1.0);
        let soft = if elapsed >= self.total_soft { Duration::ZERO } else { self.cube_soft.mul_f64(scale) };
        let hard = self.cube_hard.mul_f64(scale).min(left);
        Some(Limits { soft: soft.min(hard), hard })
    }
}

fn clock(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 60 { format!("{} min {:02} s", s / 60, s % 60) } else { format!("{:.1} s", d.as_secs_f32()) }
}

pub(crate) fn pixels(size: i32) -> u32 {
    if size <= 0 { DEFAULT_SIZE } else { (1u32 << (size - 1).min(15) as u32).clamp(MIN_SIZE, MAX_SIZE) }
}

fn samples_for(size: u32, fine: bool) -> u32 {
    let budget = if fine { RAYS_FINAL } else { RAYS_NORMAL };
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
    seed: u32,
    fine: bool,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<u8>, String> {
    bake_within(bytes, models, seed, fine, Budget::default(), log)
}

pub(crate) fn bake_within(
    bytes: Vec<u8>,
    models: &dyn ModelSource,
    seed: u32,
    fine: bool,
    budget: Budget,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<u8>, String> {
    let t = Instant::now();
    let src = hp_bsp::Bsp::from_vec(bytes).map_err(|e| format!("not a BSP file: {e}"))?;
    let cubes = src.cubemaps().unwrap_or_default();
    let Some(map) = map_name(&src) else {
        log("cubemaps: the map name is not known (no materials/maps folder in the pakfile), cubemaps not rendered");
        return Ok(src.into_vec());
    };
    log(&format!("cubemaps: reading the lit map, its materials and static props for {} env_cubemap", cubes.len()));
    models.prepare();
    let with_pak = crate::rad::props::MapPak { pak: src.pakfile().ok().filter(|p| !p.is_empty()), inner: models };
    let scene = scene::build(&src, &with_pak)?;
    drop(with_pak);
    for l in &scene.lines {
        log(l);
    }
    if !scene.passes.iter().any(|p| *p) {
        log("cubemaps: the map has no lighting, cubemaps not rendered");
        return Ok(src.into_vec());
    }
    let (jobs, shrunk) = plan(&map, &cubes, scene.passes);
    if shrunk {
        log(&format!(
            "cubemaps: the largest cubemaps are rendered at a smaller size so they fit in {} MB of the pakfile",
            PAK_BUDGET >> 20
        ));
    }
    log(&format!(
        "cubemaps: scene ready in {}: {} triangles, {} lights; rendering {} env_cubemap and the default cubemap ({})",
        clock(t.elapsed()),
        scene.tris.len(),
        scene.lights.len(),
        jobs.len() - 1,
        passes_name(scene.passes)
    ));
    let started = Instant::now();
    let mut last_note = Instant::now();
    let mut rendered: Vec<(String, Faces, u32)> = Vec::with_capacity(jobs.len());
    let mut partial = 0usize;
    let mut skipped = 0usize;
    let mut details = 0usize;
    let mut spent_noted = false;
    let total = jobs.len();
    for (i, j) in jobs.iter().enumerate() {
        let Some(o) = j.origin else {
            rendered.push((j.name.clone(), scene.render_sky(j.size), j.size));
            continue;
        };
        let samples = samples_for(j.size, fine);
        let elapsed = started.elapsed();
        let at = format!("env_cubemap {} of {} at ({} {} {}) ({} px)", i + 1, total - 1, o[0], o[1], o[2], j.size);
        let Some(limits) = budget.limits(j.size, samples, elapsed) else {
            skipped += 1;
            if details < DETAIL_LINES {
                details += 1;
                log(&format!(
                    "Warning: cubemaps: {at} skipped: the cubemap time budget ({}) is spent, it keeps its placeholder until buildcubemaps runs in game",
                    clock(budget.total_hard)
                ));
            }
            continue;
        };
        if !spent_noted && elapsed >= budget.total_soft {
            spent_noted = true;
            log(&format!(
                "Warning: cubemaps: {} spent on {} of {} cubemaps, the remaining ones get a quick pass only",
                clock(elapsed),
                i,
                total - 1
            ));
        }
        let one = Instant::now();
        let center = Vec3A::new(o[0] as f32, o[1] as f32, o[2] as f32);
        let (faces, outcome) = scene.render(center, j.size, samples, seed.wrapping_add(i as u32), limits);
        match outcome {
            Outcome::Full => rendered.push((j.name.clone(), faces, j.size)),
            Outcome::Partial { least, of } => {
                partial += 1;
                if details < DETAIL_LINES && !spent_noted {
                    details += 1;
                    log(&format!(
                        "Warning: cubemaps: {at} stopped refining after {} with {least} of {of} samples per pixel (slow surroundings: many props, lights or translucent layers)",
                        clock(one.elapsed())
                    ));
                }
                rendered.push((j.name.clone(), faces, j.size));
            }
            Outcome::Skipped { faces_done } => {
                skipped += 1;
                if details < DETAIL_LINES {
                    details += 1;
                    log(&format!(
                        "Warning: cubemaps: {at} skipped: its first pass did not finish in {} ({faces_done} of 6 faces done), it keeps its placeholder until buildcubemaps runs in game",
                        clock(limits.hard)
                    ));
                }
            }
        }
        if last_note.elapsed() >= PROGRESS_EVERY {
            last_note = Instant::now();
            log(&format!(
                "cubemaps: {} of {} env_cubemap done ({}%), {}",
                i + 1,
                total - 1,
                (i + 1) * 100 / (total - 1).max(1),
                clock(started.elapsed())
            ));
        }
    }
    if details >= DETAIL_LINES && partial + skipped > details {
        log(&format!("cubemaps: {} more cubemaps were cut short, not listed one by one", partial + skipped - details));
    }
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
    let rendered_cubes = rendered.len().saturating_sub(1);
    let mut sizes: Vec<u32> = jobs.iter().filter(|j| j.origin.is_some()).map(|j| j.size).collect();
    sizes.sort_unstable();
    sizes.dedup();
    log(&format!(
        "cubemaps: {} and the default cubemap, {} {} textures ({} KB) written to materials/maps/{map}/ in the pakfile in {:.1} s, {}",
        if sizes.is_empty() {
            "no env_cubemap in the map, only the default cubemap rendered".to_owned()
        } else {
            format!(
                "{} env_cubemap rendered ({} px)",
                rendered_cubes,
                sizes.iter().map(u32::to_string).collect::<Vec<_>>().join(", ")
            )
        },
        count,
        passes_name(scene.passes),
        added / 1024,
        t.elapsed().as_secs_f32(),
        match (partial, skipped) {
            (0, 0) => "no buildcubemaps needed".to_owned(),
            (p, 0) => format!("{p} with fewer samples, no buildcubemaps needed"),
            (0, k) => format!("{k} skipped (run buildcubemaps in game for them)"),
            (p, k) => format!("{p} with fewer samples, {k} skipped (run buildcubemaps in game for them)"),
        }
    ));
    Ok(out)
}

fn passes_name(passes: [bool; 2]) -> &'static str {
    match passes {
        [true, true] => "LDR and HDR",
        [false, true] => "HDR",
        _ => "LDR",
    }
}
