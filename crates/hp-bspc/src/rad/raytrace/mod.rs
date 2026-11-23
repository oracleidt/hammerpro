pub(crate) mod bake;
pub(crate) mod effects;
mod progressive;
pub(crate) mod reflections;
mod shafts;
#[cfg(test)]
mod tests;
mod volume;

pub use hp_raytrace::{DeviceInfo, DevicePreference, NO_GPU_MESSAGE, Params, Quality, no_gpu_message};
pub use progressive::{Progressive, Refined};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub size: f32,
}
pub use shafts::{
    DecodedVolume, SHAFT_MATERIAL, Shaft, ShaftMesh, ShaftVertex, ShaftVolume, decode_volume, encode_volume,
    light_shafts,
};

#[derive(Clone, Debug, PartialEq)]
pub struct RayTracing {
    pub quality: Quality,
    pub seed: u32,
    pub device: DevicePreference,
    pub min_samples: Option<u32>,
    pub max_samples: Option<u32>,
    pub bounces: Option<u32>,
    pub adaptive_threshold: Option<f32>,
    pub light_size_scale: Option<f32>,
    pub sun_angle: Option<f32>,
    pub sky_samples: Option<u32>,
    pub denoise: Option<f32>,
    pub supersample: Option<u32>,
    pub bump: bool,
    pub prop_samples: Option<u32>,
    pub chunk_luxels: Option<u32>,
    pub luxel_detail: Option<f32>,
    pub look: Option<Look>,
    pub lang: String,
    pub effects: Effects,
    pub gpu_share: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Look {
    Classic,
    Cinematic,
}

impl Look {
    pub fn name(self) -> &'static str {
        match self {
            Look::Classic => "classic",
            Look::Cinematic => "cinematic",
        }
    }

    pub fn parse(text: &str) -> Option<Look> {
        match text.trim().to_ascii_lowercase().as_str() {
            "classic" | "vrad" => Some(Look::Classic),
            "cinematic" | "physical" => Some(Look::Cinematic),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ShaftSource {
    Sun,
    #[default]
    SunAndSpots,
}

impl ShaftSource {
    pub fn name(self) -> &'static str {
        match self {
            ShaftSource::Sun => "sun",
            ShaftSource::SunAndSpots => "sun_and_spots",
        }
    }

    pub fn parse(text: &str) -> Option<ShaftSource> {
        match text.trim().to_ascii_lowercase().as_str() {
            "sun" => Some(ShaftSource::Sun),
            "sun_and_spots" | "spots" | "all" => Some(ShaftSource::SunAndSpots),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effects {
    pub ao: bool,
    pub ao_strength: f32,
    pub ao_radius: f32,
    pub shafts: bool,
    pub shaft_intensity: f32,
    pub shaft_length: f32,
    pub shaft_softness: f32,
    pub shaft_light_color: bool,
    pub shaft_source: ShaftSource,
    pub shaft_dust: bool,
    pub glow: bool,
    pub glow_intensity: f32,
    pub auto_cubemaps: bool,
    pub reflections: Option<bool>,
    pub tonemap: bool,
    pub color_correction: bool,
}

impl Default for Effects {
    fn default() -> Self {
        Effects {
            ao: true,
            ao_strength: 0.5,
            ao_radius: 48.0,
            shafts: false,
            shaft_intensity: 1.0,
            shaft_length: 1.0,
            shaft_softness: 1.0,
            shaft_light_color: true,
            shaft_source: ShaftSource::SunAndSpots,
            shaft_dust: false,
            glow: false,
            glow_intensity: 1.0,
            auto_cubemaps: true,
            reflections: None,
            tonemap: true,
            color_correction: false,
        }
    }
}

impl Effects {
    pub fn none() -> Effects {
        Effects {
            ao: false,
            shafts: false,
            glow: false,
            auto_cubemaps: false,
            reflections: Some(false),
            tonemap: false,
            color_correction: false,
            ..Effects::default()
        }
    }

    pub fn sanitized(mut self) -> Effects {
        let fix = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        self.ao_strength = fix(self.ao_strength, 0.0, 1.0, 0.5);
        self.ao_radius = fix(self.ao_radius, 4.0, 1024.0, 48.0);
        self.shaft_intensity = fix(self.shaft_intensity, 0.0, 8.0, 1.0);
        self.shaft_length = fix(self.shaft_length, 0.1, 8.0, 1.0);
        self.shaft_softness = fix(self.shaft_softness, 0.0, 4.0, 1.0);
        self.glow_intensity = fix(self.glow_intensity, 0.0, 8.0, 1.0);
        self
    }
}

impl Default for RayTracing {
    fn default() -> Self {
        RayTracing {
            quality: Quality::Normal,
            seed: 0,
            device: DevicePreference::Auto,
            min_samples: None,
            max_samples: None,
            bounces: None,
            adaptive_threshold: None,
            light_size_scale: None,
            sun_angle: None,
            sky_samples: None,
            denoise: None,
            supersample: None,
            bump: true,
            prop_samples: None,
            chunk_luxels: None,
            luxel_detail: None,
            look: None,
            lang: String::new(),
            effects: Effects::default(),
            gpu_share: None,
        }
    }
}

impl RayTracing {
    pub fn with_quality(quality: Quality) -> RayTracing {
        RayTracing { quality, ..RayTracing::default() }
    }

    pub fn params(&self) -> Params {
        let mut p = Params::of(self.quality);
        if let Some(v) = self.min_samples {
            p.min_samples = v;
        }
        if let Some(v) = self.max_samples {
            p.max_samples = v;
        }
        if let Some(v) = self.bounces {
            p.bounces = v;
        }
        if let Some(v) = self.adaptive_threshold {
            p.adaptive_threshold = v;
        }
        if let Some(v) = self.light_size_scale {
            p.light_size_scale = v;
        }
        if self.sun_angle.is_some() {
            p.sun_angle = self.sun_angle;
        }
        if let Some(v) = self.sky_samples {
            p.sky_samples = v;
        }
        if let Some(v) = self.denoise {
            p.denoise = v;
        }
        if let Some(v) = self.supersample {
            p.supersample = v;
        }
        if let Some(v) = self.chunk_luxels {
            p.chunk_luxels = v;
        }
        if let Some(look) = self.look {
            p.cinematic = look == Look::Cinematic;
            p.contact_ao = p.cinematic;
        }
        if p.max_samples < p.min_samples {
            p.max_samples = p.min_samples;
        }
        p.sanitized()
    }

    pub fn detail(&self) -> f32 {
        let auto = Params::of(self.quality).luxel_detail;
        let d = self.luxel_detail.unwrap_or(auto);
        if d.is_finite() { d.clamp(1.0, MAX_LUXEL_DETAIL) } else { auto }
    }

    pub(crate) fn for_detail(&self, detail: f32) -> RayTracing {
        let mut out = self.clone();
        if detail <= 1.01 {
            return out;
        }
        let p = Params::of(self.quality);
        let area = detail * detail;
        let scaled = |v: u32, floor: u32| ((v as f32 / area).round() as u32).max(floor).min(v);
        if out.min_samples.is_none() {
            out.min_samples = Some(scaled(p.min_samples, 64));
        }
        if out.max_samples.is_none() {
            out.max_samples = Some(scaled(p.max_samples, 256).max(out.min_samples.unwrap_or(1)));
        }
        if out.supersample.is_none() {
            out.supersample = Some(((p.supersample as f32 / detail).round() as u32).clamp(2, p.supersample.max(2)));
        }
        out
    }

    pub fn no_gpu_message(&self) -> &'static str {
        no_gpu_message(&self.lang)
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn bake_lightmaps(
    ctx: &super::bake::Ctx<'_>,
    faces: &[super::surface::LitFace],
    prop_tris: &[hp_light::bvh::Triangle],
    occluders: &super::props::Occluders,
    src: &hp_bsp::Bsp,
    materials: Vec<bake::MaterialInfo>,
    rt: &RayTracing,
    detail: f32,
    opts: &super::RadOptions,
    entities: &[hp_bsp::Entity],
    log: &mut dyn FnMut(&str),
) -> Result<(Vec<super::bake::FaceLight>, effects::EffectOutput), String> {
    use super::progress::Stage;
    let tuned = rt.for_detail(detail);
    let rt = &tuned;
    use std::sync::atomic::{AtomicU8, Ordering};
    let t = std::time::Instant::now();
    let sizes: Vec<(i32, i32)> = super::texlights::texdata(src).into_iter().map(|(_, w, h)| (w, h)).collect();
    let alpha = materials.iter().filter(|m| m.alpha.is_some()).count();
    ctx.work.begin(Stage::Direct, 10_000);
    let built =
        bake::build(ctx, faces, prop_tris, occluders, &materials, &sizes, rt, !opts.no_style_bounce, ctx.passes);
    let luxels = built.scene.luxels.len();
    let points = built.scene.points.len();
    log(&format!(
        "ray tracing scene: {} triangles, {} materials ({alpha} alpha tested), {} lights, {luxels} luxels, {points} sample points, built in {} ms",
        built.scene.triangles.len(),
        built.scene.materials.len(),
        built.scene.lights.len(),
        t.elapsed().as_millis()
    ));
    log(&format!(
        "ray tracing: {:.1} lights tested per luxel ({:.1} before dropping lights too faint to see, below {} at the face)",
        built.light_tests.1,
        built.light_tests.0,
        bake::LIGHT_CUTOFF
    ));
    if let Some(line) = light_size_line(&built.scene.lights) {
        log(&line);
    }
    if (built.exposure - 1.0).abs() > 0.001 {
        log(&format!(
            "ray tracing: cinematic look, light pools from area sources, exposure matched to the authored falloff (x{:.3})",
            built.exposure
        ));
    }
    let effects = rt.effects.sanitized();
    let settings = hp_raytrace::Settings {
        params: rt.params(),
        seed: rt.seed,
        style_bounce: !opts.no_style_bounce,
        device: rt.device,
        threads: opts.threads,
        vram_budget: None,
        ao_radius: if effects.ao && effects.ao_strength > 0.0 { effects.ao_radius } else { 0.0 },
        submit_seconds: 0.08,
        duty: 1.0,
    };
    ctx.work.set(SETUP_SHARE / 2);
    let mut baker = hp_raytrace::Baker::new(built.scene.clone(), settings, log)?;
    ctx.work.set(SETUP_SHARE);
    if baker.device().no_gpu {
        log(rt.no_gpu_message());
        if !rt.lang.trim().is_empty() && rt.no_gpu_message() != NO_GPU_MESSAGE {
            log(NO_GPU_MESSAGE);
        }
    }
    let (params, why) = bake::optimal_params(rt, luxels, baker.on_gpu());
    if !why.is_empty() {
        log(&why);
    }
    baker.set_params(params);
    let ss = bake::supersample_for(params.supersample, luxels) as u32;
    if ss < params.supersample {
        log(&format!(
            "ray tracing: {}x{} sample points per luxel instead of {}x{} so the sample points of {luxels} luxels stay in memory",
            ss, ss, params.supersample, params.supersample
        ));
    }
    log(&format!(
        "ray tracing on {}: {} to {} samples per luxel (adaptive, threshold {}), {} bounces, {}x{} sample points per luxel, denoiser {}",
        baker.device().describe(),
        params.min_samples,
        params.max_samples,
        params.adaptive_threshold,
        params.bounces,
        ss,
        ss,
        params.denoise
    ));
    let work = ctx.work;
    let phase = AtomicU8::new(1);
    let progress = |p: hp_raytrace::Progress| {
        let code = match p.phase {
            hp_raytrace::Phase::Upload | hp_raytrace::Phase::Direct => 1,
            _ => 2,
        };
        if phase.swap(code, Ordering::AcqRel) != code {
            work.begin(if code == 1 { Stage::Direct } else { Stage::Bounces }, 10_000);
        }
        let local = if code == 1 { p.fraction / 0.45 } else { (p.fraction - 0.45) / 0.55 };
        let floor = if code == 1 { SETUP_SHARE } else { 0 };
        work.set(floor + (local.clamp(0.0, 1.0) * (10_000 - floor) as f32) as u64);
    };
    let ctl = hp_raytrace::Control { cancelled: &|| false, progress: &progress };
    let out = baker.bake(&ctl, log).map_err(|e| e.to_string())?;
    log(&format!(
        "ray tracing done in {:.1} s ({} direct samples, {} paths)",
        out.seconds, out.direct_samples, out.indirect_samples
    ));
    let strength = contact_strengths(rt.params().cinematic, if effects.ao { effects.ao_strength } else { 0.0 });
    let hdr = opts.hdr;
    let fx = effects::build(&baker.tracer(), baker.scene(), entities, &effects, hdr, ctx.passes.saturating_sub(1));
    for l in &fx.lines {
        log(l);
    }
    let lift = bake::contact_exposure(&out, &built, strength.0);
    if lift > 1.001 {
        log(&format!("ray tracing: contact shadows, typical luxel kept at its exposure (direct light x{lift:.3})"));
    }
    let mut lights = bake::to_face_lights(&out, &built, ctx.passes, strength);
    if lift > 1.001 {
        for l in &mut lights {
            for v in &mut l.values {
                *v *= lift;
            }
        }
    }
    Ok((lights, fx))
}

const SETUP_SHARE: u64 = 200;

pub(crate) fn contact_strengths(cinematic: bool, strength: f32) -> (f32, f32) {
    if cinematic { (strength * CONTACT_DIRECT, (strength * 2.0).min(1.0)) } else { (0.0, strength) }
}

pub(crate) const CONTACT_DIRECT: f32 = 1.2;

pub const PREVIEW_SUBMIT_SECONDS: f64 = 0.008;
pub const PREVIEW_AUTO_SHARE: f32 = 0.6;

pub(crate) const OPTIMAL_LIGHTING_BUDGET: u64 = 48 << 20;

pub(crate) fn lighting_estimate(src: &hp_bsp::Bsp) -> Result<u64, String> {
    let map = super::data::MapData::read(src)?;
    let mut bytes = 0u64;
    for f in &map.faces {
        let flags = map.face_flags(f);
        if flags & (crate::materials::surf::NOLIGHT | crate::materials::surf::SKY) != 0 {
            continue;
        }
        let lux = (f.size[0].max(0) as u64 + 1) * (f.size[1].max(0) as u64 + 1);
        let normals = if flags & crate::materials::surf::BUMPLIGHT != 0 { 4 } else { 1 };
        bytes += lux * 4 * normals + 4;
    }
    Ok(bytes)
}

pub(crate) const MAX_LUXEL_DETAIL: f32 = 4.0;
pub(crate) const FINAL_LIGHTING_BUDGET: u64 = 32 << 20;

fn light_size_line(lights: &[hp_raytrace::Light]) -> Option<String> {
    let mut sizes: Vec<f32> = lights
        .iter()
        .filter(|l| matches!(l.kind, hp_raytrace::LightKind::Point | hp_raytrace::LightKind::Spot { .. }))
        .map(|l| l.size)
        .collect();
    if sizes.is_empty() {
        return None;
    }
    sizes.sort_by(f32::total_cmp);
    let sun = lights.iter().find(|l| l.kind == hp_raytrace::LightKind::Sun).map(|l| l.size.asin().to_degrees());
    Some(format!(
        "ray tracing: area lights {:.1} to {:.1} units in radius (median {:.1}), fitted to the room around each light{}",
        sizes[0],
        sizes[sizes.len() - 1],
        sizes[sizes.len() / 2],
        sun.map(|a| format!(", sun disk {a:.2} degrees")).unwrap_or_default()
    ))
}

pub(crate) fn fit_lightmaps(
    src: hp_bsp::Bsp,
    opts: &super::RadOptions,
    log: &mut dyn FnMut(&str),
) -> Result<(hp_bsp::Bsp, f32), String> {
    let Some(rt) = &opts.raytracing else { return Ok((src, 1.0)) };
    let detail = rt.detail();
    if detail > 1.01 && !(opts.luxel_density > 0.0 && opts.luxel_density < 1.0) {
        return refine_lightmaps(src, opts, detail, log);
    }
    if rt.quality != Quality::Optimal {
        return Ok((src, 1.0));
    }
    let limit = hp_bsp::limits::LIGHTING.max_in(opts.limits).min(OPTIMAL_LIGHTING_BUDGET);
    let estimate = lighting_estimate(&src)?;
    let factor = style_factor(&src);
    let styled = (estimate as f64 * factor) as u64;
    if styled <= limit {
        log(&format!(
            "Optimal ray tracing: lightmaps about {} KB per pass, {:.0}% of the {} KB budget: lightmap scales kept",
            styled / 1024,
            styled as f64 * 100.0 / limit as f64,
            limit / 1024
        ));
        return Ok((src, 1.0));
    }
    let density = ((limit as f64 * 0.9 / styled as f64).sqrt() as f32).clamp(0.25, 0.99);
    let scaled = super::output::rescale_luxels(src, density)?;
    let out = hp_bsp::Bsp::from_vec(scaled).map_err(|e| format!("not a BSP file: {e}"))?;
    let after = (lighting_estimate(&out)? as f64 * factor) as u64;
    log(&format!(
        "Optimal ray tracing: lightmaps would take about {} KB per pass, over the {} KB budget: luxel density scaled to {density:.2}, now about {} KB ({:.0}% of the budget)",
        styled / 1024,
        limit / 1024,
        after / 1024,
        after as f64 * 100.0 / limit as f64
    ));
    Ok((out, 1.0))
}

fn refine_lightmaps(
    src: hp_bsp::Bsp,
    opts: &super::RadOptions,
    detail: f32,
    log: &mut dyn FnMut(&str),
) -> Result<(hp_bsp::Bsp, f32), String> {
    let limit = hp_bsp::limits::LIGHTING.max_in(opts.limits).min(FINAL_LIGHTING_BUDGET);
    let factor = style_factor(&src);
    let styled = (lighting_estimate(&src)? as f64 * factor) as u64;
    let room = (limit as f64 * 0.9 / styled.max(1) as f64).sqrt() as f32;
    let d = detail.min(room);
    if d < 1.1 {
        log(&format!(
            "ray tracing: lightmaps already take about {} KB per pass of the {} KB budget: luxel detail kept",
            styled / 1024,
            limit / 1024
        ));
        return Ok((src, 1.0));
    }
    let max_extent = hp_bsp::limits::LIGHTMAP_EXTENT.max_in(opts.limits) as i32;
    let max_texinfo = hp_bsp::limits::TEXINFO.max_in(opts.limits).min(i16::MAX as u64) as usize;
    let refined = match super::output::refine_luxels(src, d, max_extent, max_texinfo)? {
        Ok(r) => r,
        Err(src) => {
            log("ray tracing: no face can take finer lightmaps within the lightmap size limit: luxel detail kept");
            return Ok((src, 1.0));
        }
    };
    let out = hp_bsp::Bsp::from_vec(refined.bytes).map_err(|e| format!("not a BSP file: {e}"))?;
    let after = (lighting_estimate(&out)? as f64 * factor) as u64;
    log(&format!(
        "ray tracing: finer lightmaps, luxel detail x{d:.2} on {} faces ({} kept at their size by the lightmap size limit, {} texinfos added): about {} KB per pass instead of {} KB",
        refined.faces,
        refined.kept,
        refined.texinfos,
        after / 1024,
        styled / 1024
    ));
    Ok((out, d))
}

pub(crate) fn prop_ao(rt: &RayTracing) -> Option<super::AoOptions> {
    let fx = rt.effects.sanitized();
    (fx.ao && fx.ao_strength > 0.0).then_some(super::AoOptions {
        radius: fx.ao_radius,
        scale: fx.ao_strength,
        face_samples: None,
        prop_samples: rt.prop_samples,
        debug: false,
    })
}

pub(crate) fn tuned_options(opts: &super::RadOptions) -> super::RadOptions {
    let mut out = opts.clone();
    let Some(rt) = &opts.raytracing else { return out };
    let scale = match rt.prop_samples {
        Some(n) => (n.max(16) as f32 / 162.0).clamp(0.1, 64.0),
        None => match rt.quality {
            Quality::Draft => 1.0,
            Quality::Normal | Quality::Optimal => 2.0,
            Quality::Final => 4.0,
        },
    };
    out.props.sample_scale = out.props.sample_scale.max(scale);
    out.props.sky_sample_scale = out.props.sky_sample_scale.max(scale);
    if rt.quality == Quality::Final {
        out.quality = super::RadQuality::Final;
    }
    if matches!(rt.quality, Quality::Final | Quality::Optimal) {
        out.props.polys = true;
        out.props.texture_shadows = true;
    }
    out
}

pub(crate) const SELFILLUM_BRIGHTNESS: f32 = 300.0;

pub(crate) fn add_selfillum(
    tables: &mut [super::texlights::LightFileData],
    src: &hp_bsp::Bsp,
    materials: &[bake::MaterialInfo],
) -> usize {
    let names = super::texlights::texdata(src);
    let mut added = 0;
    for (i, m) in materials.iter().enumerate() {
        let (Some(glow), Some((name, _, _))) = (m.selfillum, names.get(i)) else { continue };
        let value = glam::Vec3A::from_array(glow) * SELFILLUM_BRIGHTNESS;
        let mut used = false;
        for t in tables.iter_mut() {
            if t.texlights.iter().any(|l| l.name.eq_ignore_ascii_case(name)) {
                continue;
            }
            t.texlights.push(super::texlights::TexLight { name: name.clone(), value, file: usize::MAX });
            used = true;
        }
        added += usize::from(used);
    }
    added
}

fn style_factor(src: &hp_bsp::Bsp) -> f64 {
    let named = src.entities().ok().is_some_and(|ents| {
        ents.iter()
            .any(|e| e.classname().starts_with("light") && e.get("targetname").is_some_and(|n| !n.trim().is_empty()))
    });
    if named { 1.75 } else { 1.15 }
}
