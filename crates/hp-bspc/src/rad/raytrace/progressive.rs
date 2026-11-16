use std::sync::Arc;
use std::time::Instant;

use super::super::RadOptions;
use super::super::bake::{self, FaceLight};
use super::super::lights::Lights;
use super::super::session::{self, Control, FaceOut, Phase, Scene, Stop};
use super::{NO_GPU_MESSAGE, RayTracing};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Refined {
    pub faces: Vec<FaceOut>,
    pub progress: f32,
    pub samples_per_luxel: f32,
    pub finished: bool,
    pub seconds: f64,
}

pub struct Progressive {
    scene: Arc<Scene>,
    built: super::bake::Built,
    baker: hp_raytrace::Baker,
    rt: RayTracing,
    opts: RadOptions,
    lights: Lights,
    styled_entities: Vec<hp_bsp::Entity>,
    focus: Option<Vec<u32>>,
    face_of_surface: Vec<u32>,
    surface_of_face: std::collections::HashMap<u32, u32>,
    ao_strength: f32,
    materials: Vec<super::bake::MaterialInfo>,
    sizes: Vec<(i32, i32)>,
}

fn engine_control<'a>(
    ctl: &'a Control<'a>,
    phase: Phase,
) -> (impl Fn() -> bool + Sync + 'a, impl Fn(hp_raytrace::Progress) + Sync + 'a) {
    let cancelled = move || (ctl.cancelled)();
    let progress = move |p: hp_raytrace::Progress| (ctl.progress)(phase, p.fraction);
    (cancelled, progress)
}

fn preview_submit_seconds() -> f64 {
    std::env::var("HAMMERPRO_RT_PREVIEW_SUBMIT_MS")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.0)
        .map_or(super::PREVIEW_SUBMIT_SECONDS, |ms| ms / 1000.0)
}

fn stop(e: hp_raytrace::Stop) -> Stop {
    match e {
        hp_raytrace::Stop::Cancelled => Stop::Cancelled,
        hp_raytrace::Stop::Failed(m) => Stop::Failed(m),
    }
}

impl Progressive {
    pub fn new(
        bsp: Vec<u8>,
        opts: &RadOptions,
        models: &dyn crate::props::ModelSource,
        log: &mut dyn FnMut(&str),
        ctl: &Control<'_>,
    ) -> Result<Progressive, Stop> {
        let rt = opts.raytracing.clone().unwrap_or_default();
        let mut opts = opts.clone();
        opts.raytracing = Some(rt.clone());
        let scene = Arc::new(Scene::build(bsp, &opts, models, log, ctl)?);
        if (ctl.cancelled)() {
            return Err(Stop::Cancelled);
        }
        let with_pak =
            super::super::props::MapPak { pak: scene.src.pakfile().ok().filter(|p| !p.is_empty()), inner: models };
        let read = |p: &str| crate::props::ModelSource::read_file(&with_pak, p);
        let materials = super::bake::world_materials(&scene.src, &read);
        let sizes: Vec<(i32, i32)> =
            super::super::texlights::texdata(&scene.src).into_iter().map(|(_, w, h)| (w, h)).collect();
        let (styled_entities, _, lights) = session::scene_lights(&scene, &scene.map.entities, &opts);
        let built = Self::build(&scene, &lights, &rt, &opts, &materials, &sizes);
        let effects = rt.effects.sanitized();
        let settings = hp_raytrace::Settings {
            params: rt.params(),
            seed: rt.seed,
            style_bounce: !opts.no_style_bounce,
            device: rt.device,
            threads: opts.threads,
            vram_budget: None,
            ao_radius: if effects.ao && effects.ao_strength > 0.0 { effects.ao_radius } else { 0.0 },
            submit_seconds: preview_submit_seconds(),
            duty: rt.gpu_share.unwrap_or(super::PREVIEW_AUTO_SHARE),
        };
        let luxels = built.scene.luxels.len();
        let mut baker = hp_raytrace::Baker::new(built.scene.clone(), settings, log).map_err(Stop::Failed)?;
        if baker.device().no_gpu {
            log(rt.no_gpu_message());
            if rt.no_gpu_message() != NO_GPU_MESSAGE {
                log(NO_GPU_MESSAGE);
            }
        }
        let (params, why) = super::bake::optimal_params(&rt, luxels, baker.on_gpu());
        if !why.is_empty() {
            log(&why);
        }
        baker.set_params(params);
        let share = rt.gpu_share.unwrap_or(super::PREVIEW_AUTO_SHARE);
        baker.set_pacing(preview_submit_seconds(), share);
        log(&format!("ray traced preview on {}", baker.device().describe()));
        let face_of_surface: Vec<u32> = scene.faces.iter().map(|f| f.face as u32).collect();
        let surface_of_face = face_of_surface.iter().enumerate().map(|(i, f)| (*f, i as u32)).collect();
        Ok(Progressive {
            scene,
            built,
            baker,
            rt,
            opts,
            lights,
            styled_entities,
            focus: None,
            face_of_surface,
            surface_of_face,
            ao_strength: if effects.ao { effects.ao_strength } else { 0.0 },
            materials,
            sizes,
        })
    }

    fn build(
        scene: &Scene,
        lights: &Lights,
        rt: &RayTracing,
        opts: &RadOptions,
        materials: &[super::bake::MaterialInfo],
        sizes: &[(i32, i32)],
    ) -> super::bake::Built {
        let work = super::super::progress::Work::new();
        let ctx = session::make_ctx(scene, lights, session::params(opts), &work, opts.bounce_budget);
        super::bake::build(
            &ctx,
            &scene.faces,
            &scene.rt_prop_tris,
            &scene.occluders,
            materials,
            sizes,
            rt,
            !opts.no_style_bounce,
            scene.passes.len(),
        )
    }

    pub fn device(&self) -> &hp_raytrace::DeviceInfo {
        self.baker.device()
    }

    pub fn faces(&self) -> usize {
        self.scene.faces.len()
    }

    pub fn luxels(&self) -> usize {
        self.scene.luxels
    }

    pub fn styled_entities(&self) -> &[hp_bsp::Entity] {
        &self.styled_entities
    }

    pub fn set_focus(&mut self, faces: Option<Vec<u32>>) {
        self.focus = faces.map(|list| {
            let mut v: Vec<u32> = list.iter().filter_map(|f| self.surface_of_face.get(f).copied()).collect();
            v.sort_unstable();
            v.dedup();
            v
        });
    }

    pub fn set_entities(&mut self, entities: &[hp_bsp::Entity], log: &mut dyn FnMut(&str)) -> Result<(), Stop> {
        let t = Instant::now();
        let (styled, _, lights) = session::scene_lights(&self.scene, entities, &self.opts);
        let built = Self::build(&self.scene, &lights, &self.rt, &self.opts, &self.materials, &self.sizes);
        let ranges: Vec<(u32, u32)> = built.scene.surfaces.iter().map(|s| s.lights).collect();
        let styles_changed =
            built.scene.surfaces.iter().zip(&self.built.scene.surfaces).any(|(a, b)| a.styles != b.styles);
        if styles_changed {
            let mut fresh =
                hp_raytrace::Baker::new(built.scene.clone(), self.baker_settings(), log).map_err(Stop::Failed)?;
            fresh.set_params(self.baker.params());
            self.baker = fresh;
        } else {
            self.baker
                .set_lights(built.scene.lights.clone(), built.scene.light_lists.clone(), ranges)
                .map_err(Stop::Failed)?;
        }
        self.built = built;
        self.lights = lights;
        self.styled_entities = styled;
        log(&format!("ray traced preview: lights updated in {} ms", t.elapsed().as_millis()));
        Ok(())
    }

    fn baker_settings(&self) -> hp_raytrace::Settings {
        let effects = self.rt.effects.sanitized();
        hp_raytrace::Settings {
            params: self.baker.params(),
            seed: self.rt.seed,
            style_bounce: !self.opts.no_style_bounce,
            device: self.rt.device,
            threads: self.opts.threads,
            vram_budget: None,
            ao_radius: if effects.ao && effects.ao_strength > 0.0 { effects.ao_radius } else { 0.0 },
            submit_seconds: preview_submit_seconds(),
            duty: self.rt.gpu_share.unwrap_or(super::PREVIEW_AUTO_SHARE),
        }
    }

    fn face_outs(&self, surfaces: &[u32]) -> Vec<FaceOut> {
        let passes = self.scene.passes.len();
        let pass = passes.saturating_sub(1);
        let outs = self.baker.output_surfaces(surfaces, true);
        outs.into_iter()
            .map(|(si, so)| {
                let si = si as usize;
                let mut fl: Vec<FaceLight> =
                    vec![super::bake::face_light(&so, &self.built, si, passes, self.ao_strength)];
                let face = &self.scene.faces[si];
                bake::fill_dark(std::slice::from_ref(face), &mut fl);
                let l = fl.pop().unwrap_or_default();
                let values = session::widen(&l.values);
                session::face_out(face, &l, values, pass)
            })
            .collect()
    }

    pub fn refine(&mut self, ctl: &Control<'_>, seconds: f32) -> Result<Refined, Stop> {
        let (cancelled, progress) = engine_control(ctl, Phase::Direct);
        let ectl = hp_raytrace::Control { cancelled: &cancelled, progress: &progress };
        let r = self.baker.refine(&ectl, seconds, self.focus.as_deref()).map_err(stop)?;
        let faces = self.face_outs(&r.surfaces);
        Ok(Refined {
            faces,
            progress: r.progress,
            samples_per_luxel: r.samples_per_luxel,
            finished: r.finished,
            seconds: r.seconds,
        })
    }

    pub fn output_all(&self) -> Vec<FaceOut> {
        let passes = self.scene.passes.len();
        let pass = passes.saturating_sub(1);
        let out = self.baker.output(true);
        let mut lights = super::bake::to_face_lights(&out, &self.built, passes, self.ao_strength);
        bake::fill_dark(&self.scene.faces, &mut lights);
        bake::stitch(&self.scene.faces, &mut lights);
        self.scene
            .faces
            .iter()
            .zip(&lights)
            .map(|(f, l)| session::face_out(f, l, session::widen(&l.values), pass))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn set_pacing_for_tests(&mut self, submit: f64, share: f32) {
        self.baker.set_pacing(submit, share);
    }

    pub fn set_gpu_share(&mut self, share: Option<f32>) {
        self.rt.gpu_share = share;
        self.baker.set_pacing(preview_submit_seconds(), share.unwrap_or(super::PREVIEW_AUTO_SHARE));
    }

    pub fn finished(&self) -> bool {
        self.baker.finished()
    }

    pub fn models_job(&self) -> session::ModelsJob {
        let passes = self.scene.passes.len();
        let out = self.baker.output(true);
        let mut total = super::bake::to_face_lights(&out, &self.built, passes, self.ao_strength);
        bake::fill_dark(&self.scene.faces, &mut total);
        bake::stitch(&self.scene.faces, &mut total);
        session::ModelsJob::of(
            self.scene.clone(),
            self.lights.clone(),
            session::params(&self.opts),
            total,
            self.styled_entities.clone(),
        )
    }

    pub fn glows(&self) -> Vec<super::Glow> {
        let fx = self.rt.effects.sanitized();
        if !fx.glow {
            return Vec::new();
        }
        let pass = self.scene.passes.len().saturating_sub(1);
        let numbers = |e: &hp_bsp::Entity, k: &str| -> Vec<f32> {
            e.get(k).unwrap_or_default().split_whitespace().filter_map(|v| v.parse().ok()).collect()
        };
        super::effects::glow_entities(self.baker.scene(), &self.styled_entities, &fx, pass)
            .iter()
            .filter_map(|e| {
                let o = e.origin()?;
                let c = numbers(e, "rendercolor");
                let scale = numbers(e, "scale").first().copied()?;
                (c.len() == 3).then(|| super::Glow {
                    position: [o[0] as f32, o[1] as f32, o[2] as f32],
                    color: [c[0] / 255.0, c[1] / 255.0, c[2] / 255.0],
                    size: scale * super::effects::GLOW_TEXTURE.0 as f32,
                })
            })
            .collect()
    }

    pub fn light_shafts(&self) -> super::ShaftMesh {
        let pass = self.scene.passes.len().saturating_sub(1);
        super::effects::shaft_mesh(&self.baker.tracer(), self.baker.scene(), &self.rt.effects, pass)
    }

    pub fn face_index(&self, surface: usize) -> Option<u32> {
        self.face_of_surface.get(surface).copied()
    }
}
