use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::Instant;

use glam::Vec3;
use hp_bspc::rad::RayTracing;
use hp_bspc::rad::raytrace::Progressive;
use hp_bspc::rad::session::{Control, Phase, Stop};

use super::engine::{Cancelled, Engine, Out, RadModels, convert, phase_code, say};
use super::{LitStats, RayOptions, RayReport, Reply};

const FOCUS_RADIUS: f32 = 2048.0;
const FOCUS_PASSES: u32 = 2;
const FIRST_SLICE: f32 = 0.35;
const LONGEST_SLICE: f32 = 0.6;

pub(crate) struct RayWarm {
    key: u64,
    settings: RayTracing,
    lights: u64,
    entities: Vec<hp_bsp::Entity>,
    pub(super) progressive: Progressive,
    centers: Vec<Vec3>,
    shafts: Vec<u8>,
    pass: u32,
    seconds: f32,
    samples: f32,
}

pub(crate) fn shafts_of(progressive: &Progressive, settings: &RayTracing) -> Vec<u8> {
    let glows: Vec<super::GlowRecord> = progressive.glows().iter().map(|g| (g.position, g.color, g.size)).collect();
    if !settings.effects.shafts {
        return super::encode_shafts(&[], &[], &glows);
    }
    let mesh = progressive.light_shafts();
    let vertices: Vec<_> = mesh.vertices.iter().map(|v| (v.pos, v.uv, v.color)).collect();
    super::encode_shafts(&vertices, &mesh.indices, &glows)
}

pub(crate) fn settings_of(o: &RayOptions) -> Result<RayTracing, String> {
    let mut args = o.args.clone();
    if !args.iter().any(|a| a.eq_ignore_ascii_case("-raytracing")) {
        args.insert(0, "-raytracing".to_owned());
    }
    args.push("preview".to_owned());
    let parsed = crate::bspc_cli::parse_rad_args(&args)?;
    Ok(parsed.opts.raytracing.unwrap_or_default())
}

fn lights_key(entities: &[hp_bsp::Entity]) -> u64 {
    let mut hash = DefaultHasher::new();
    for e in entities {
        e.pairs.hash(&mut hash);
    }
    hash.finish()
}

pub(crate) fn focus_faces(centers: &[Vec3], points: &[Vec3]) -> Option<Vec<u32>> {
    let near: Vec<u32> = centers
        .iter()
        .enumerate()
        .filter(|(_, c)| points.iter().any(|p| c.distance_squared(*p) <= FOCUS_RADIUS * FOCUS_RADIUS))
        .map(|(i, _)| i as u32)
        .collect();
    (!near.is_empty() && near.len() < centers.len()).then_some(near)
}

fn entity_key(e: &hp_bsp::Entity, index: usize) -> String {
    e.get("hammerid").map_or_else(|| format!("#{index}"), str::to_owned)
}

pub(crate) fn edited_points(old: &[hp_bsp::Entity], new: &[hp_bsp::Entity]) -> Option<Vec<Vec3>> {
    use std::collections::HashMap;
    let index = |list: &[hp_bsp::Entity]| -> HashMap<String, usize> {
        list.iter().enumerate().map(|(i, e)| (entity_key(e, i), i)).collect()
    };
    let (before, after) = (index(old), index(new));
    let mut points = Vec::new();
    let mut note = |e: &hp_bsp::Entity| -> bool {
        if e.classname().eq_ignore_ascii_case("light_environment") || e.classname().eq_ignore_ascii_case("worldspawn") {
            return false;
        }
        if let Some(o) = e.origin() {
            points.push(Vec3::new(o[0] as f32, o[1] as f32, o[2] as f32));
        }
        true
    };
    for (k, &i) in &before {
        match after.get(k) {
            Some(&j) if old[i].pairs == new[j].pairs => {}
            Some(&j) => {
                if !note(&old[i]) || !note(&new[j]) {
                    return None;
                }
            }
            None => {
                if !note(&old[i]) {
                    return None;
                }
            }
        }
    }
    for (k, &j) in &after {
        if !before.contains_key(k) && !note(&new[j]) {
            return None;
        }
    }
    (!points.is_empty()).then_some(points)
}

pub(crate) fn slice_for(pass: u32) -> f32 {
    (FIRST_SLICE * 1.5f32.powi(pass as i32)).min(LONGEST_SLICE)
}

const REFOCUS_DISTANCE: f32 = FOCUS_RADIUS * 0.5;

pub(crate) fn moved_far(seen: Option<[f32; 3]>, now: [f32; 3]) -> bool {
    seen.is_some_and(|s| Vec3::from_array(s).distance(Vec3::from_array(now)) > REFOCUS_DISTANCE)
}

const YIELD_STEP: std::time::Duration = std::time::Duration::from_millis(15);

fn yield_to_editor(cancelled: Cancelled<'_>) -> bool {
    while super::server::INTERACTIVE.load(std::sync::atomic::Ordering::Acquire) {
        if cancelled() {
            return false;
        }
        std::thread::sleep(YIELD_STEP);
    }
    true
}

fn device_of(p: &Progressive) -> (String, bool) {
    let d = p.device();
    match &d.gpu {
        Some(g) => (g.name.clone(), true),
        None => (format!("{} threads", d.threads.max(1)), false),
    }
}

impl Engine {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn relight_ray(
        &mut self,
        job: u64,
        removed: &[u32],
        blob: &[u8],
        options: &super::LightOptions,
        focus: Option<[f32; 3]>,
        out: Out<'_>,
        cancelled: Cancelled<'_>,
    ) -> Result<(), Stop> {
        let started = Instant::now();
        self.apply_entities(removed, blob)?;
        let ray = options.raytracing.clone().ok_or_else(|| Stop::Failed("no ray tracing options".into()))?;
        let settings = settings_of(&ray)?;
        let mut opts = self.rad_options(options);
        opts.raytracing = Some(settings.clone());
        let fs = self.fs();
        let models = RadModels { fs: &fs };
        let geometry = self.geometry.as_ref().ok_or_else(|| Stop::Failed("no geometry".into()))?;
        let lights = lights_key(&geometry.entities);
        let last = Mutex::new((Instant::now(), None::<Phase>));
        let progress = |phase: Phase, fraction: f32| {
            if let Ok(mut l) = last.lock()
                && (l.1 != Some(phase) || l.0.elapsed().as_millis() >= 150)
            {
                *l = (Instant::now(), Some(phase));
                out(Reply::Progress { job, phase: phase_code(phase), fraction }, Vec::new());
            }
        };
        let ctl = Control { cancelled, progress: &progress, partial: None };
        let same_scene = self.ray.as_ref().is_some_and(|w| w.key == geometry.key && w.settings == settings);
        let reuse = same_scene && self.ray.as_ref().is_some_and(|w| w.lights == lights);
        let mut scene_seconds = 0.0f32;
        let mut points: Vec<Vec3> = focus.map(Vec3::from_array).into_iter().collect();
        if same_scene
            && !reuse
            && let Some(w) = self.ray.as_mut()
        {
            let t = Instant::now();
            if let Some(edited) = edited_points(&w.entities, &geometry.entities) {
                points = edited;
            }
            w.progressive.set_entities(&geometry.entities, &mut |l| say(out, l))?;
            w.shafts = shafts_of(&w.progressive, &settings);
            w.lights = lights;
            w.entities = geometry.entities.clone();
            w.pass = 0;
            w.seconds = 0.0;
            w.samples = 0.0;
            scene_seconds = t.elapsed().as_secs_f32();
        } else if !reuse {
            self.ray = None;
            let t = Instant::now();
            let mut edit = hp_bsp::Bsp::from_vec(geometry.bsp.clone()).map_err(|e| e.to_string())?.edit();
            edit.set_entities(&geometry.entities).map_err(|e| e.to_string())?;
            let bsp = edit.to_vec().map_err(|e| e.to_string())?;
            let centers =
                hp_bsp::Bsp::from_vec(bsp.clone()).map(|b| super::super::bsp::face_centers(&b)).unwrap_or_default();
            let progressive = Progressive::new(bsp, &opts, &models, &mut |l| say(out, l), &ctl)?;
            let shafts = shafts_of(&progressive, &settings);
            scene_seconds = t.elapsed().as_secs_f32();
            self.ray = Some(RayWarm {
                key: geometry.key,
                settings,
                lights,
                entities: geometry.entities.clone(),
                progressive,
                centers,
                shafts,
                pass: 0,
                seconds: 0.0,
                samples: 0.0,
            });
        }
        let warm = self.ray.as_mut().ok_or_else(|| Stop::Failed("no ray tracing session".into()))?;
        let (device, gpu) = device_of(&warm.progressive);
        let params = opts.raytracing.as_ref().map(RayTracing::params);
        let target = params.map_or(0, |p| p.max_samples);
        let denoised = params.is_some_and(|p| p.denoise > 0.0);
        let report = |warm: &RayWarm, refined: Option<&hp_bspc::rad::raytrace::Refined>, faces: usize| RayReport {
            device: device.clone(),
            gpu,
            pass: warm.pass,
            progress: refined.map_or(0.0, |r| r.progress),
            samples: refined.map_or(warm.samples, |r| r.samples_per_luxel).round().max(0.0) as u32,
            target_samples: target,
            denoised,
            converged: warm.progressive.finished(),
            seconds: warm.seconds,
            faces,
        };
        out(Reply::Raytrace { job, report: report(warm, None, 0) }, Vec::new());
        out(Reply::Shafts { job }, warm.shafts.clone());
        let mut focused = focus_faces(&warm.centers, &points);
        let mut seen_camera = focus.or_else(|| super::server::CAMERA.lock().ok().and_then(|c| *c));
        let mut sent = 0usize;
        let mut local_until = FOCUS_PASSES;
        while !warm.progressive.finished() {
            if cancelled() {
                return Err(Stop::Cancelled);
            }
            if !yield_to_editor(cancelled) {
                return Err(Stop::Cancelled);
            }
            let camera = super::server::CAMERA.lock().ok().and_then(|c| *c);
            if let Some(c) = camera.filter(|c| moved_far(seen_camera, *c)) {
                seen_camera = Some(c);
                focused = focus_faces(&warm.centers, &[Vec3::from_array(c)]);
                local_until = warm.pass + FOCUS_PASSES;
            }
            let local = warm.pass < local_until;
            warm.progressive.set_focus(if local { focused.clone() } else { None });
            let refined = warm.progressive.refine(&ctl, slice_for(warm.pass))?;
            warm.pass += 1;
            warm.seconds += refined.seconds as f32;
            warm.samples = refined.samples_per_luxel;
            if !refined.faces.is_empty() {
                sent = refined.faces.len();
                let converted = convert(&refined.faces, warm.progressive.styled_entities(), options.hdr);
                out(Reply::Partial { job, pass: warm.pass }, super::encode_faces(&converted));
            }
            out(Reply::Raytrace { job, report: report(warm, Some(&refined), sent) }, Vec::new());
            if refined.finished {
                break;
            }
        }
        let all = warm.progressive.output_all();
        let converted = convert(&all, warm.progressive.styled_entities(), options.hdr);
        let final_report = RayReport { converged: true, progress: 1.0, ..report(warm, None, all.len()) };
        out(Reply::Raytrace { job, report: final_report }, Vec::new());
        let stats = LitStats {
            full: true,
            lights: 0,
            changed_lights: 0,
            relit_faces: all.len(),
            faces: warm.progressive.faces(),
            luxels: warm.progressive.luxels() as u64,
            triangles: 0,
            scene_seconds,
            direct_seconds: warm.seconds,
            bounce_seconds: 0.0,
            seconds: started.elapsed().as_secs_f32(),
            session_mb: 0,
            process_mb: super::super::memory::of_self().map_or(0, |u| u.working_set >> 20),
            skybox: None,
        };
        out(Reply::Lit { job, stats }, super::encode_faces(&converted));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_bspc::rad::{RtDevice, RtQuality};

    fn ent(pairs: &[(&str, &str)]) -> hp_bsp::Entity {
        let mut e = hp_bsp::Entity::default();
        for (k, v) in pairs {
            e.set(k, (*v).to_owned());
        }
        e
    }

    #[test]
    fn an_edit_relights_the_area_it_reaches_first() {
        let world = ent(&[("classname", "worldspawn")]);
        let lamp =
            ent(&[("classname", "light"), ("hammerid", "5"), ("origin", "100 0 0"), ("_light", "255 255 255 200")]);
        let moved =
            ent(&[("classname", "light"), ("hammerid", "5"), ("origin", "900 0 0"), ("_light", "255 255 255 200")]);
        let old = vec![world.clone(), lamp.clone()];
        assert_eq!(edited_points(&old, &old), None, "nothing changed");
        let points = edited_points(&old, &[world.clone(), moved]).unwrap();
        assert_eq!(points, vec![Vec3::new(100.0, 0.0, 0.0), Vec3::new(900.0, 0.0, 0.0)]);
        let added = ent(&[("classname", "light_spot"), ("hammerid", "8"), ("origin", "0 50 0")]);
        let points = edited_points(&old, &[world.clone(), lamp.clone(), added]).unwrap();
        assert_eq!(points, vec![Vec3::new(0.0, 50.0, 0.0)]);
        let sun = ent(&[("classname", "light_environment"), ("hammerid", "9"), ("angles", "0 0 0")]);
        assert_eq!(edited_points(&old, &[world, lamp, sun]), None, "the sun lights everything");
    }

    #[test]
    fn preview_settings_are_the_compile_settings() {
        let mut compile = hp_app::settings::CompileSettings::default();
        let default = settings_of(&RayOptions { args: hp_ui_mapping::raytracing_preview_args(&compile) }).unwrap();
        assert_eq!(default.quality, RtQuality::Normal, "the Default configuration");
        assert_eq!(default.params(), RayTracing::default().params());
        compile.raytracing.quality = hp_app::settings::RayQualitySetting::Final;
        compile.raytracing.bounces = Some(9);
        compile.raytracing.denoise = Some(0.0);
        compile.raytracing.device = hp_app::settings::RayDeviceSetting::Cpu;
        compile.raytracing.seed = 5;
        compile.raytracing.bump = false;
        compile.raytracing.shafts = true;
        let args = hp_ui_mapping::raytracing_preview_args(&compile);
        assert!(!compile.raytracing.on, "the preview does not need the compile toggle");
        let s = settings_of(&RayOptions { args: args.clone() }).unwrap();
        assert_eq!((s.quality, s.device, s.seed, s.bump), (RtQuality::Final, RtDevice::Cpu, 5, false));
        let p = s.params();
        assert_eq!((p.bounces, p.denoise), (9, 0.0));
        assert!(s.effects.shafts, "the effect options of the compile");
        let plain: Vec<String> = args.into_iter().filter(|a| a != "-raytracing").collect();
        assert_eq!(settings_of(&RayOptions { args: plain }).unwrap(), s);
        compile.raytracing.make_optimal();
        let optimal = settings_of(&RayOptions { args: hp_ui_mapping::raytracing_preview_args(&compile) }).unwrap();
        assert_eq!(optimal.quality, RtQuality::Optimal);
    }

    #[test]
    fn tracing_waits_while_the_editor_is_interactive() {
        use std::sync::atomic::Ordering;
        super::super::server::INTERACTIVE.store(true, Ordering::Release);
        assert!(!yield_to_editor(&|| true), "a cancel ends the wait");
        super::super::server::INTERACTIVE.store(false, Ordering::Release);
        assert!(yield_to_editor(&|| false));
        assert!(!moved_far(None, [0.0; 3]));
        assert!(!moved_far(Some([0.0; 3]), [100.0, 0.0, 0.0]));
        assert!(moved_far(Some([0.0; 3]), [FOCUS_RADIUS, 0.0, 0.0]), "a far camera gets its area first again");
    }

    #[test]
    fn the_camera_area_comes_first_then_everything() {
        let centers = vec![Vec3::ZERO, Vec3::new(100.0, 0.0, 0.0), Vec3::new(9000.0, 0.0, 0.0)];
        assert_eq!(focus_faces(&centers, &[Vec3::ZERO]), Some(vec![0, 1]));
        assert_eq!(focus_faces(&centers, &[]), None);
        assert_eq!(focus_faces(&centers[..2], &[Vec3::ZERO]), None, "everything is near: no focus");
        assert_eq!(focus_faces(&centers, &[Vec3::new(9100.0, 0.0, 0.0)]), Some(vec![2]));
        assert!(slice_for(0) < slice_for(3) && slice_for(30) == LONGEST_SLICE);
    }
}
