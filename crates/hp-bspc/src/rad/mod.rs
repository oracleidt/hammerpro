mod ambient;
mod bake;
mod data;
mod detail;
mod disp;
mod lights;
mod output;
pub mod progress;
mod props;
mod sky;
mod surface;
#[cfg(test)]
mod tests;
mod texlights;

use std::time::Instant;

use rayon::prelude::*;

use glam::Vec3A;

use crate::emit::lump;
use crate::materials::{contents, surf};
pub use ambient::ProbeOptions;
pub use output::check_lighting;
pub use props::PropOptions;
pub use texlights::{LightFile, LightFileData, TexLight, TexLightOptions};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RadQuality {
    Draft,
    #[default]
    Normal,
    Final,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadOptions {
    pub hdr: bool,
    pub ldr: bool,
    pub bounces: u32,
    pub quality: RadQuality,
    pub threads: Option<usize>,
    pub texlights: TexLightOptions,
    pub props: PropOptions,
    pub no_extra: bool,
    pub extra_sky: Option<f32>,
    pub smooth_angle: f64,
    pub soft_sun: f32,
    pub no_skybox_recurse: bool,
    pub fast_ambient: bool,
    pub luxel_density: f32,
    pub no_disp_shadows: bool,
    pub min_sun_samples: usize,
    pub normal_sampling: bool,
    pub probes: Option<ProbeOptions>,
    pub limits: hp_bsp::limits::LimitSet,
}

impl Default for RadOptions {
    fn default() -> Self {
        Self {
            hdr: true,
            ldr: true,
            bounces: 100,
            quality: RadQuality::Normal,
            threads: None,
            texlights: TexLightOptions::default(),
            props: PropOptions::default(),
            no_extra: false,
            extra_sky: None,
            smooth_angle: 45.0,
            soft_sun: 0.0,
            no_skybox_recurse: false,
            fast_ambient: false,
            luxel_density: 1.0,
            no_disp_shadows: false,
            min_sun_samples: 0,
            normal_sampling: false,
            probes: None,
            limits: hp_bsp::limits::LimitSet::Gmod,
        }
    }
}

pub fn light_bsp(bsp: &[u8], opts: &RadOptions, log: &mut dyn FnMut(&str)) -> Result<Vec<u8>, String> {
    light_bsp_with(bsp, opts, &crate::props::NoModels, log)
}

pub fn light_bsp_with(
    bsp: &[u8],
    opts: &RadOptions,
    models: &dyn crate::props::ModelSource,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<u8>, String> {
    light_bsp_with_report(bsp, opts, models, log, &mut crate::stats::RadReport::default())
}

pub fn light_bsp_report(
    bsp: &[u8],
    opts: &RadOptions,
    models: &dyn crate::props::ModelSource,
    log: &mut dyn FnMut(&str),
) -> Result<(Vec<u8>, crate::stats::RadReport), String> {
    let mut report = crate::stats::RadReport::default();
    let out = light_bsp_with_report(bsp, opts, models, log, &mut report)?;
    Ok((out, report))
}

fn light_bsp_with_report(
    bsp: &[u8],
    opts: &RadOptions,
    models: &dyn crate::props::ModelSource,
    log: &mut dyn FnMut(&str),
    report: &mut crate::stats::RadReport,
) -> Result<Vec<u8>, String> {
    if !opts.hdr && !opts.ldr {
        return Err("nothing to light: neither LDR nor HDR asked".to_owned());
    }
    let pool = match opts.threads {
        Some(n) => Some(
            rayon::ThreadPoolBuilder::new()
                .num_threads(n.max(1))
                .build()
                .map_err(|e| format!("cannot start the threads: {e}"))?,
        ),
        None => None,
    };
    let runner = Runner { pool: pool.as_ref() };
    report.limits = opts.limits;
    let work = progress::Work::new();
    progress::watch(&work, log, |log| run_with(bsp, opts, &runner, models, log, report, &work))
}

struct Runner<'a> {
    pool: Option<&'a rayon::ThreadPool>,
}

impl Runner<'_> {
    fn par<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        match self.pool {
            Some(p) => p.install(f),
            None => f(),
        }
    }
}

fn ms(t: Instant) -> u128 {
    t.elapsed().as_millis()
}

fn stage(report: &mut crate::stats::RadReport, name: &str, t: Instant) {
    report.stages.push((name.to_owned(), t.elapsed().as_secs_f64()));
}

fn run_with(
    bytes: &[u8],
    opts: &RadOptions,
    runner: &Runner<'_>,
    models: &dyn crate::props::ModelSource,
    log: &mut dyn FnMut(&str),
    report: &mut crate::stats::RadReport,
    work: &progress::Work,
) -> Result<Vec<u8>, String> {
    use progress::Stage;
    let start = Instant::now();
    work.begin(Stage::Scene, 8);
    let mut src = hp_bsp::Bsp::from_vec(bytes.to_vec()).map_err(|e| format!("not a BSP file: {e}"))?;
    if opts.luxel_density > 0.0 && opts.luxel_density < 1.0 {
        let scaled = output::rescale_luxels(src, opts.luxel_density)?;
        src = hp_bsp::Bsp::from_vec(scaled).map_err(|e| format!("not a BSP file: {e}"))?;
        log(&format!("luxels rescaled by {} (-luxeldensity)", opts.luxel_density));
    }
    if !(19..=21).contains(&src.version()) {
        return Err(format!("BSP version {} is not supported", src.version()));
    }
    let map = data::MapData::read(&src)?;
    let with_pak = props::MapPak { pak: src.pakfile().ok().filter(|p| !p.is_empty()), inner: models };
    let models: &dyn crate::props::ModelSource = &with_pak;
    work.add(1);
    let mut entities = map.entities.clone();
    let styled = lights::assign_styles(&mut entities, hp_bsp::limits::SWITCHED_LIGHTS.max_usize_in(opts.limits));
    let worldlights_version = src.lump_info(15).map_or(0, |l| l.version);
    let passes: Vec<bool> = [(opts.ldr, false), (opts.hdr, true)].iter().filter(|p| p.0).map(|p| p.1).collect();
    let mut lights = lights::parse(&entities, &passes, &map);
    if lights.sun_extent <= 0.0 && opts.soft_sun > 0.0 {
        lights.sun_extent = opts.soft_sun.to_radians().sin().max(0.0);
    }
    let t = Instant::now();
    let tables = passes
        .iter()
        .map(|&hdr| texlights::LightFileData::read(&opts.texlights.files, hdr, log))
        .collect::<Result<Vec<_>, _>>()?;
    let mut emission =
        texlights::emitters(&src, &map, &tables, &opts.texlights, opts.bounces > 0 && !opts.texlights.fast);
    if !opts.texlights.files.is_empty() {
        log(&format!(
            "{} texture light faces: {} surface lights in {} ms",
            emission.faces,
            emission.lights.len(),
            ms(t)
        ));
    }
    stage(report, "texture lights", t);
    report.texture_light_faces = emission.faces;
    lights.list.extend(std::mem::take(&mut emission.lights));
    work.add(1);
    let mut params = bake::Params::of(opts.quality);
    if opts.no_extra {
        params.supersample_passes = 0;
    }
    if let Some(n) = opts.extra_sky.filter(|n| n.is_finite() && *n > 0.0) {
        params.sky_dirs = ((162.0 * n).round() as usize).clamp(16, 162 * 32);
    }
    if opts.fast_ambient {
        params.leaf_samples = 1;
    }
    params.sun_samples = params.sun_samples.max(opts.min_sun_samples);
    if opts.normal_sampling {
        params = params.at_least(&bake::Params::of(RadQuality::Normal));
    }

    let t = Instant::now();
    let all = runner.par(|| surface::lit_faces(&map, opts.smooth_angle));
    let faces: Vec<surface::LitFace> = all.iter().flatten().cloned().collect();
    let lit_index = bake::lit_index(map.faces.len(), &faces);
    work.add(1);
    let rules_table = tables.first();
    let no_shadow = |m: &str| rules_table.is_some_and(|t| t.casts_no_shadow(m));
    let forced = |m: &str| rules_table.is_some_and(|t| t.texture_shadow_forced(m));
    let rules = props::ShadowRules { no_shadow: &no_shadow, forced: &forced };
    let shadows = opts.props.shadows;
    let what = props::ReadWhat { collision: shadows, texture_shadows: opts.props.texture_shadows };
    let (mut static_props, mut prop_lines) = if shadows || opts.props.lighting {
        runner.par(|| props::StaticProps::read(&src, models, what, &rules))?
    } else {
        (props::StaticProps::default(), Vec::new())
    };
    work.add(1);
    let (prop_tris, occluders) = if shadows {
        runner.par(|| static_props.occluders(opts.props.polys, &rules, models, &mut prop_lines))
    } else {
        (Vec::new(), props::Occluders::default())
    };
    work.add(1);
    for l in &prop_lines {
        log(l);
    }
    if !shadows {
        log("static props cast no shadow (-nostaticpropshadows)");
    } else if static_props.len() > 0 {
        log(&format!("{} static props: {} shadow triangles", static_props.len(), prop_tris.len()));
    }
    let no_shadow_disps = map.disps.iter().filter(|d| d.no_shadow).count();
    if opts.no_disp_shadows {
        log("displacements cast no shadow (-nodispshadows)");
    } else if no_shadow_disps > 0 {
        log(&format!("{no_shadow_disps} displacement(s) cast no shadow (their cast shadows option is off)"));
    }
    let (shadow, _) = runner.par(|| surface::shadow_scene(&map, &all, prop_tris, !opts.no_disp_shadows));
    work.add(1);
    let (surf, surf_info) = runner.par(|| surface::surface_scene(&map, &all, &lit_index));
    work.add(1);
    let blockers = runner.par(|| surface::blocker_scene(&map));
    work.add(1);
    drop(all);
    let sky_clusters = surface::sky_clusters(&map);
    let luxels: usize = faces.iter().map(|f| f.luxels()).sum();
    if luxels > 1 << 26 {
        return Err(format!("{luxels} luxels: the lightmap sizes of the faces are broken"));
    }
    let names: Vec<&str> = passes.iter().map(|h| if *h { "HDR" } else { "LDR" }).collect();
    log(&format!(
        "{} faces, {} with lightmaps ({luxels} luxels), {} lights, {} shadow triangles, passes: {}",
        map.faces.len(),
        faces.len(),
        lights.list.len(),
        shadow.len(),
        names.join(", ")
    ));
    if styled {
        log("named lights got switchable light styles (32 and up)");
    }
    log(&format!("scene built in {} ms", ms(t)));
    stage(report, "scene", t);
    report.passes = names.iter().map(|n| (*n).to_owned()).collect();

    let (cameras, camera_areas) =
        if opts.no_skybox_recurse { (Vec::new(), Vec::new()) } else { bake::sky_cameras(&map) };
    let detail_objects = detail::count(&src);
    let prop_lighting = opts.props.lighting && static_props.len() > 0;
    let mut plan = vec![Stage::Scene, Stage::Direct];
    if opts.bounces > 0 {
        plan.extend([Stage::BounceRays, Stage::Bounces]);
    }
    plan.push(Stage::LeafAmbient);
    if detail_objects > 0 {
        plan.push(Stage::DetailProps);
    }
    if prop_lighting {
        plan.push(Stage::StaticProps);
    }
    plan.push(Stage::Write);
    work.plan(&plan);
    log(&format!("Lighting stages: {}", plan[1..].iter().map(|s| s.name()).collect::<Vec<_>>().join(", ")));
    let ctx = bake::Ctx {
        map: &map,
        faces: &faces,
        shadow: &shadow,
        surf: &surf,
        surf_info: &surf_info,
        lights: &lights,
        passes: passes.len(),
        params,
        light_rows: bake::Ctx::rows(&map, &lights, &sky_clusters),
        sky_dirs: bake::sphere_dirs(params.sky_dirs),
        sun_dirs: bake::sphere_dirs(params.sun_samples.max(1)),
        cameras,
        camera_areas,
        props: &occluders,
        blockers: &blockers,
        work,
    };
    let t = Instant::now();
    work.begin(Stage::Direct, luxels as u64);
    let mut light: Vec<bake::FaceLight> = runner.par(|| {
        faces
            .par_iter()
            .map(|f| {
                let l = ctx.direct(f);
                work.add(f.luxels() as u64);
                l
            })
            .collect()
    });
    runner.par(|| disp::radial(&ctx, &mut light));
    log(&format!("direct light in {} ms", ms(t)));
    stage(report, "direct light", t);
    let t = Instant::now();
    {
        let unbounced = (opts.bounces > 0).then(|| runner.par(|| disp::before_bounce(&ctx, &light))).flatten();
        let mut lines = Vec::new();
        let (gather, bounce_passes) =
            runner.par(|| ctx.bounce(&mut light, opts.bounces, &mut |l| lines.push(l.to_owned())));
        for l in &lines {
            log(l);
        }
        if opts.bounces > 0 {
            report.stages.push(("bounced light: gather rays".to_owned(), gather));
            report.stages.push(("bounced light: passes".to_owned(), bounce_passes));
        }
        report.bounces = lines.iter().filter(|l| l.starts_with("Bounce #")).count() as u32;
        if let Some(saved) = unbounced {
            runner.par(|| disp::after_bounce(&ctx, &mut light, saved));
        }
    }
    if opts.bounces > 0 {
        log(&format!("bounced light in {} ms", ms(t)));
        stage(report, "bounced light", t);
    }

    runner.par(|| bake::stitch(&faces, &mut light));

    let t = Instant::now();
    work.begin(Stage::LeafAmbient, map.leafs.len() as u64);
    let per_face: Vec<(Vec<Vec<glam::Vec3A>>, Vec<glam::Vec3A>)> = runner.par(|| {
        faces
            .par_iter()
            .zip(&light)
            .map(|(f, l)| {
                let lux = f.luxels();
                let luxels: Vec<Vec<glam::Vec3A>> = (0..passes.len())
                    .map(|p| {
                        l.values[l.index(p, 0, 0, lux)..l.index(p, 0, 0, lux) + lux]
                            .iter()
                            .map(|v| output::finish(*v, f.minlight))
                            .collect()
                    })
                    .collect();
                let average = luxels.iter().map(|v| output::median(v)).collect();
                (luxels, average)
            })
            .collect()
    });
    let (luxels, average) = per_face.into_iter().unzip();
    let colors = ambient::FaceColors { luxels, average };
    let ambient = runner.par(|| ambient::leaf_ambient(&ctx, &colors));
    let probes = opts.probes.as_ref().and_then(|po| runner.par(|| ambient::probe_volume(&ctx, &colors, po)));
    if let Some(v) = &probes {
        log(&format!("light probes: {} cells of {} units", v.cell_count(), v.spacing));
    }
    let samples: usize = ambient.first().map_or(0, |a| a.1.len() / 28);
    log(&format!("leaf ambient: {samples} samples in {} ms", ms(t)));
    stage(report, "leaf ambient", t);
    let t = Instant::now();
    let detail = runner.par(|| detail::light(&src, &ctx, &light, models, &passes))?;
    if let Some((_, n)) = &detail {
        log(&format!("detail props: {n} lit in {} ms", ms(t)));
        stage(report, "detail props", t);
        report.detail_lit = *n;
    }
    let t = Instant::now();
    let prop_files = if opts.props.lighting && static_props.len() > 0 {
        let bounced = opts.bounces > 0 && map.vis.clusters > 0;
        let (files, n, vertices) =
            runner.par(|| props::light(&ctx, &light, &static_props, &opts.props, bounced, &passes));
        log(&format!("static props: {n} with vertex lighting ({vertices} vertices) in {} ms", ms(t)));
        stage(report, "static prop vertex lighting", t);
        report.props_vertex_lit = n;
        files
    } else {
        Vec::new()
    };
    let pak = if prop_files.is_empty() { None } else { Some(props::add_to_pak(&src, prop_files)?) };
    let map_flags = props::map_flags(
        &src.lump(lump::MAP_FLAGS).map(|c| c.into_owned()).unwrap_or_default(),
        opts.props.lighting,
        &passes,
    );

    let t = Instant::now();
    work.begin(Stage::Write, 1);
    let layout = output::layout(&faces, &light);
    lightmap_room(layout.size as u64, hp_bsp::limits::LIGHTING.max_in(opts.limits))?;
    let faces_lump = output::patch_faces(&map.faces_raw, &layout);
    let leaf_flags = {
        let raw = src.lump(lump::LEAFS).map(|c| c.into_owned()).unwrap_or_default();
        let size = if src.lump_info(lump::LEAFS).map_or(1, |l| l.version) == 0 { 56 } else { 32 };
        let sees_sky = |l: &data::Leaf| {
            let center = Vec3A::new(
                ((l.mins[0] + l.maxs[0]) * 0.5) as f32,
                ((l.mins[1] + l.maxs[1]) * 0.5) as f32,
                ((l.mins[2] + l.maxs[2]) * 0.5) as f32,
            );
            let mut seen = SkySeen::default();
            for n in &detail::ANORMS {
                match ctx.trace_sky_kind(center, -Vec3A::from_array(*n)) {
                    Some(false) => {
                        seen.sky3d = true;
                        break;
                    }
                    Some(true) => seen.sky2d = true,
                    None => {}
                }
            }
            seen
        };
        (leaf_sky_flags(&raw, size, &map, &sees_sky), src.lump_info(lump::LEAFS).map_or(1, |l| l.version))
    };
    let raw = |i: usize| src.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let (vert_normals, vert_indices) =
        crate::emit::vertex_normals(&raw(lump::PLANES), &raw(lump::FACES), &raw(lump::SURFEDGES), &raw(lump::EDGES));
    let texinfo_lump = (!emission.light_texinfos.is_empty()).then(|| {
        let mut texinfo = raw(lump::TEXINFO);
        for &t in &emission.light_texinfos {
            if let Some(b) = texinfo.get_mut(t * 72 + 64..t * 72 + 68) {
                let flags = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) | crate::materials::surf::LIGHT;
                b.copy_from_slice(&flags.to_le_bytes());
            }
        }
        (texinfo, src.lump_info(lump::TEXINFO).map_or(0, |l| l.version))
    });
    let dispinfo_lump = crate::disp::strip_no_shadow(&raw(lump::DISPINFO))
        .map(|d| (d, src.lump_info(lump::DISPINFO).map_or(0, |l| l.version)));
    let mut edit = src.edit();
    let fail = |e: hp_bsp::BspError| e.to_string();
    edit.replace_lump_with_version(lump::VERTNORMALS, vert_normals, 0).map_err(fail)?;
    edit.replace_lump_with_version(lump::VERTNORMALINDICES, vert_indices, 0).map_err(fail)?;
    if let Some((texinfo, version)) = texinfo_lump {
        edit.replace_lump_with_version(lump::TEXINFO, texinfo, version).map_err(fail)?;
    }
    let (leafs, leafs_version) = leaf_flags;
    edit.replace_lump_with_version(lump::LEAFS, leafs, leafs_version).map_err(fail)?;
    for (p, &hdr) in passes.iter().enumerate() {
        let data = output::lighting(&faces, &light, &layout, p);
        let (faces_i, light_i, wl_i, idx_i, amb_i) = if hdr {
            (lump::FACES_HDR, lump::LIGHTING_HDR, 54, lump::LEAF_AMBIENT_INDEX_HDR, lump::LEAF_AMBIENT_LIGHTING_HDR)
        } else {
            (lump::FACES, lump::LIGHTING, 15, lump::LEAF_AMBIENT_INDEX, lump::LEAF_AMBIENT_LIGHTING)
        };
        let faces_version = if map.faces_version == 0 { 1 } else { map.faces_version };
        edit.replace_lump_with_version(faces_i, faces_lump.clone(), faces_version).map_err(fail)?;
        edit.replace_lump_with_version(light_i, data, 1).map_err(fail)?;
        let records = lights::worldlights(&lights, p);
        let records = if worldlights_version >= 1 { hp_bsp::format::worldlights_v0_to_v1(&records) } else { records };
        edit.replace_lump_with_version(wl_i, records, worldlights_version).map_err(fail)?;
        let (index, samples) = ambient.get(p).cloned().unwrap_or_default();
        edit.replace_lump_with_version(idx_i, index, 0).map_err(fail)?;
        edit.replace_lump_with_version(amb_i, samples, 1).map_err(fail)?;
    }
    if let [only] = passes[..] {
        let (index, samples) = ambient.first().cloned().unwrap_or_default();
        let (idx_i, amb_i) = if only {
            (lump::LEAF_AMBIENT_INDEX, lump::LEAF_AMBIENT_LIGHTING)
        } else {
            (lump::LEAF_AMBIENT_INDEX_HDR, lump::LEAF_AMBIENT_LIGHTING_HDR)
        };
        edit.replace_lump_with_version(idx_i, index, 0).map_err(fail)?;
        edit.replace_lump_with_version(amb_i, samples, 1).map_err(fail)?;
    }
    if styled {
        edit.set_entities(&entities).map_err(fail)?;
    }
    if let Some((games, _)) = detail {
        edit.set_game_lumps(games);
    }
    if let Some(v) = &probes {
        edit.replace_lump_with_version(hp_light::probes::LUMP, v.encode(), 0).map_err(fail)?;
    }
    edit.replace_lump_with_version(lump::MAP_FLAGS, map_flags, 0).map_err(fail)?;
    if let Some(pak) = pak {
        edit.set_pakfile(pak).map_err(fail)?;
    }
    if let Some((d, version)) = dispinfo_lump {
        edit.replace_lump_with_version(lump::DISPINFO, d, version).map_err(fail)?;
    }
    let out = edit.to_vec().map_err(fail)?;
    work.finish();
    stage(report, "write", t);
    log(&format!("lighting: {} bytes per pass, done in {} ms", layout.size, ms(start)));
    Ok(out)
}

fn lightmap_room(size: u64, most: u64) -> Result<(), String> {
    if size <= most {
        return Ok(());
    }
    Err(format!(
        "{} bytes of lightmaps per pass: a face can only point {most} bytes into the lightmaps, the game cannot load \
             this map; raise the lightmap scale of large faces",
        size
    ))
}

const LEAF_FLAGS_SKY: u16 = 0x01;
const LEAF_FLAGS_RADIAL: u16 = 0x02;
const LEAF_FLAGS_SKY2D: u16 = 0x04;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct SkySeen {
    sky3d: bool,
    sky2d: bool,
}

fn leaf_sky_flags(raw: &[u8], size: usize, map: &data::MapData, sees_sky: &dyn Fn(&data::Leaf) -> SkySeen) -> Vec<u8> {
    let mut out = raw.to_vec();
    let n = map.leafs.len().min(raw.len() / size);
    let get = |out: &[u8], i: usize| u16::from_le_bytes([out[i * size + 6], out[i * size + 7]]);
    let flags_of = |v: u16| v >> 9;
    let set = |out: &mut [u8], i: usize, flags: u16| {
        let v = (get(out, i) & 0x1ff) | (flags << 9);
        out[i * size + 6..i * size + 8].copy_from_slice(&v.to_le_bytes());
    };
    for i in 0..n {
        let leaf = &map.leafs[i];
        let mut f = flags_of(get(&out, i)) & !(LEAF_FLAGS_SKY | LEAF_FLAGS_SKY2D);
        for k in leaf.first_face..leaf.first_face + leaf.num_faces {
            let Some(face) = map.leaffaces.get(k).and_then(|x| map.faces.get(usize::from(*x))) else { continue };
            let flags = map.face_flags(face);
            if flags & surf::SKY != 0 {
                f |= if flags & surf::SKY2D != 0 { LEAF_FLAGS_SKY2D } else { LEAF_FLAGS_SKY };
                break;
            }
        }
        set(&mut out, i, f);
    }
    let sees = |from: i32, to: i32| {
        if to < 0 {
            return true;
        }
        map.vis.row(from).is_none_or(|r| r.get(to as usize >> 3).is_none_or(|b| b & (1 << (to & 7)) != 0))
    };
    let sky_leafs: Vec<(usize, u16, i32)> = (0..n)
        .map(|k| (k, flags_of(get(&out, k)), map.leafs[k].cluster))
        .filter(|(_, f, _)| f & (LEAF_FLAGS_SKY | LEAF_FLAGS_SKY2D) != 0)
        .collect();
    let scan = |i: usize, cluster: i32| {
        let (mut sky3d, mut sky2d) = (false, false);
        for &(k, f, c) in &sky_leafs {
            if k == i || !sees(cluster, c) {
                continue;
            }
            if f & LEAF_FLAGS_SKY2D != 0 {
                sky2d = true;
            }
            if f & LEAF_FLAGS_SKY != 0 {
                sky3d = true;
                break;
            }
        }
        (sky3d, sky2d)
    };
    let mut by_cluster: std::collections::HashMap<i32, (bool, bool)> = std::collections::HashMap::new();
    let (mut bits3d, mut bits2d) = (vec![false; n], vec![false; n]);
    for i in 0..n {
        let leaf = &map.leafs[i];
        let own = flags_of(get(&out, i));
        if own & LEAF_FLAGS_SKY != 0 || leaf.contents & contents::SOLID as i32 != 0 {
            continue;
        }
        let (sky3d, sky2d) = if own & LEAF_FLAGS_SKY2D != 0 {
            scan(i, leaf.cluster)
        } else {
            *by_cluster.entry(leaf.cluster).or_insert_with(|| scan(usize::MAX, leaf.cluster))
        };
        bits3d[i] = sky3d;
        bits2d[i] = sky2d;
    }
    for i in 0..n {
        let leaf = &map.leafs[i];
        let mut f = flags_of(get(&out, i));
        if f & LEAF_FLAGS_SKY != 0 || leaf.contents & contents::SOLID as i32 != 0 {
            continue;
        }
        if bits2d[i] {
            f |= LEAF_FLAGS_SKY2D;
        }
        if bits3d[i] {
            f = (f | LEAF_FLAGS_SKY) & !LEAF_FLAGS_SKY2D;
        } else if f & LEAF_FLAGS_RADIAL != 0 {
            let seen = sees_sky(leaf);
            if seen.sky3d {
                f = (f | LEAF_FLAGS_SKY) & !LEAF_FLAGS_SKY2D;
            } else if seen.sky2d {
                f |= LEAF_FLAGS_SKY2D;
            }
        }
        set(&mut out, i, f);
    }
    out
}
