pub(crate) mod bake;
pub(crate) mod effects;
mod progressive;
mod shafts;
#[cfg(test)]
mod tests;

pub use hp_raytrace::{DeviceInfo, DevicePreference, NO_GPU_MESSAGE, Params, Quality, no_gpu_message};
pub use progressive::{Progressive, Refined};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub position: [f32; 3],
    pub color: [f32; 3],
    pub size: f32,
}
pub use shafts::{SHAFT_MATERIAL, Shaft, ShaftMesh, ShaftVertex, light_shafts};

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
    pub lang: String,
    pub effects: Effects,
    pub gpu_share: Option<f32>,
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
    pub tonemap: bool,
    pub color_correction: bool,
}

impl Default for Effects {
    fn default() -> Self {
        Effects {
            ao: true,
            ao_strength: 0.5,
            ao_radius: 48.0,
            shafts: true,
            shaft_intensity: 1.0,
            shaft_length: 1.0,
            shaft_softness: 1.0,
            shaft_light_color: true,
            shaft_source: ShaftSource::SunAndSpots,
            shaft_dust: false,
            glow: false,
            glow_intensity: 1.0,
            auto_cubemaps: true,
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
        if p.max_samples < p.min_samples {
            p.max_samples = p.min_samples;
        }
        p.sanitized()
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
    opts: &super::RadOptions,
    entities: &[hp_bsp::Entity],
    log: &mut dyn FnMut(&str),
) -> Result<(Vec<super::bake::FaceLight>, effects::EffectOutput), String> {
    use super::progress::Stage;
    use std::sync::atomic::{AtomicU8, Ordering};
    let t = std::time::Instant::now();
    let sizes: Vec<(i32, i32)> = super::texlights::texdata(src).into_iter().map(|(_, w, h)| (w, h)).collect();
    let alpha = materials.iter().filter(|m| m.alpha.is_some()).count();
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
    let mut baker = hp_raytrace::Baker::new(built.scene.clone(), settings, log)?;
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
    let phase = AtomicU8::new(0);
    let progress = |p: hp_raytrace::Progress| {
        let code = match p.phase {
            hp_raytrace::Phase::Upload | hp_raytrace::Phase::Direct => 1,
            _ => 2,
        };
        if phase.swap(code, Ordering::AcqRel) != code {
            work.begin(if code == 1 { Stage::Direct } else { Stage::Bounces }, 10_000);
        }
        let local = if code == 1 { p.fraction / 0.45 } else { (p.fraction - 0.45) / 0.55 };
        work.set((local.clamp(0.0, 1.0) * 10_000.0) as u64);
    };
    let ctl = hp_raytrace::Control { cancelled: &|| false, progress: &progress };
    let out = baker.bake(&ctl, log).map_err(|e| e.to_string())?;
    log(&format!(
        "ray tracing done in {:.1} s ({} direct samples, {} paths)",
        out.seconds, out.direct_samples, out.indirect_samples
    ));
    let strength = if effects.ao { effects.ao_strength } else { 0.0 };
    let hdr = opts.hdr;
    let fx = effects::build(&baker.tracer(), baker.scene(), entities, &effects, hdr, ctx.passes.saturating_sub(1));
    for l in &fx.lines {
        log(l);
    }
    Ok((bake::to_face_lights(&out, &built, ctx.passes, strength), fx))
}

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

pub(crate) fn fit_lightmaps(
    src: hp_bsp::Bsp,
    opts: &super::RadOptions,
    log: &mut dyn FnMut(&str),
) -> Result<hp_bsp::Bsp, String> {
    let Some(rt) = &opts.raytracing else { return Ok(src) };
    if rt.quality != Quality::Optimal {
        return Ok(src);
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
        return Ok(src);
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
    Ok(out)
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
