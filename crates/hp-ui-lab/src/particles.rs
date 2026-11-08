use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use glam::DVec3;
use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_editor::{Id, ObjRef};
use hp_pcf::{Effect, Plan, Points, Render, RenderKind, Sheet};
use hp_render::{Overlay, OverlayTri, ParticleBlend, ParticleOrient};
use hp_ui_viewports::layers::OverlayLayer;

pub const MAX_EFFECTS: usize = 24;
pub const MAX_PARTICLES: usize = MAX_PER_EFFECT * MAX_EFFECTS;
pub const MAX_PER_EFFECT: usize = hp_pcf::sim::MAX_EFFECT_PARTICLES;
pub const MAX_DISTANCE: f64 = 4096.0;
const STARTS_PER_FRAME: usize = 4;
const WARM_UP: f32 = 1.5;
const HOVER_PX: f32 = 14.0;

const BSP_ID_BASE: Id = 0xF000_0000;

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialLook {
    pub found: bool,
    pub spritecard: bool,
    pub additive: bool,
    pub overbright: f32,
    pub add_self: f32,
    pub fade_size: [f32; 2],
    pub opacity: f32,
    pub blend_frames: bool,
    pub dual_sequence: bool,
    pub sequence_mode: u8,
    pub max_luminance: [bool; 2],
    pub sheet: Option<Sheet>,
    pub texture_height: f32,
    pub refraction: Option<f32>,
    pub refraction_blur: f32,
    pub no_fog: bool,
}

impl Default for MaterialLook {
    fn default() -> Self {
        Self {
            found: false,
            spritecard: false,
            additive: false,
            overbright: 1.0,
            add_self: 0.0,
            fade_size: [0.0; 2],
            opacity: 1.0,
            blend_frames: false,
            dual_sequence: false,
            sequence_mode: 0,
            max_luminance: [false; 2],
            sheet: None,
            texture_height: 1.0,
            refraction: None,
            refraction_blur: 0.0,
            no_fog: false,
        }
    }
}

impl MaterialLook {
    pub fn blend(&self) -> ParticleBlend {
        if self.refraction.is_some() {
            ParticleBlend::Refract
        } else if self.additive {
            ParticleBlend::Additive
        } else if self.add_self > 0.0 {
            ParticleBlend::AddSelf
        } else {
            ParticleBlend::Alpha
        }
    }

    pub fn brightness(&self) -> f32 {
        self.overbright
    }

    pub fn blend_parameter(&self) -> f32 {
        self.refraction.unwrap_or_else(|| if self.blend() == ParticleBlend::AddSelf { self.add_self } else { 0.0 })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectStatus {
    Loaded,
    Loading,
    NotFound,
}

impl EffectStatus {
    pub fn id(self) -> &'static str {
        match self {
            EffectStatus::Loaded => "loaded",
            EffectStatus::Loading => "loading",
            EffectStatus::NotFound => "not_found",
        }
    }
}

#[derive(Debug, Default)]
pub struct ParticleShared {
    pub(crate) models: Arc<crate::particle_world::models::Models>,
    pub(crate) model_requirement: HashSet<Id>,
    pub physics: Arc<crate::particle_world::Physics>,
    pub(crate) rays: Arc<crate::particle_world::rays::RayWorld>,
    pub(crate) ray_requirement: bool,
    pub(crate) ray_preparation: crate::particle_world::rays::Preparation,
    pub(crate) world_requirement: Option<bool>,
    pub(crate) world_preparation: crate::particle_world::Preparation,
    pub library: HashMap<String, Arc<Plan>>,
    pub library_gen: u64,
    pub files: HashMap<String, String>,
    pub looks: HashMap<String, Arc<MaterialLook>>,
    pub wanted: HashSet<String>,
    pub missing: HashSet<String>,
    pub active: bool,
    pub fx_active: bool,
    pub sprite_layer: bool,
    pub sprites: Vec<ParticleSprite>,
    pub model_draws: Vec<hp_app::state::ParticleModelDraw>,
    pub materials: Vec<String>,
    pub fx_sprites: Vec<ParticleSprite>,
    pub fx_materials: Vec<String>,
    pub playing: Vec<(Id, String)>,
    pub particles: usize,
}

impl ParticleShared {
    fn model_control_points(&self, name: &str, collisions: bool) -> Vec<i32> {
        let mut todo = vec![name.to_ascii_lowercase()];
        let mut seen = HashSet::new();
        let mut points = Vec::new();
        while let Some(name) = todo.pop() {
            if !seen.insert(name.clone()) || seen.len() > 256 {
                continue;
            }
            let Some(plan) = self.library.get(&name) else { continue };
            points.extend(if collisions { plan.model_collision_control_points() } else { plan.model_control_points() });
            todo.extend(plan.children.iter().map(|(child, _)| child.to_ascii_lowercase()));
        }
        points.sort_unstable();
        points.dedup();
        points
    }
    fn trace_requirement(&self, name: &str) -> Option<bool> {
        let mut todo = vec![name.to_ascii_lowercase()];
        let mut seen = HashSet::new();
        let mut requirement = None;
        while let Some(name) = todo.pop() {
            if !seen.insert(name.clone()) || seen.len() > 256 {
                continue;
            }
            let Some(plan) = self.library.get(&name) else { continue };
            requirement = requirement.max(plan.trace_requirement());
            todo.extend(plan.children.iter().map(|(child, _)| child.to_ascii_lowercase()));
        }
        requirement
    }

    fn uses_ray_environments(&self, name: &str) -> bool {
        let mut todo = vec![name.to_ascii_lowercase()];
        let mut seen = HashSet::new();
        while let Some(name) = todo.pop() {
            if !seen.insert(name.clone()) || seen.len() > 256 {
                continue;
            }
            let Some(plan) = self.library.get(&name) else { continue };
            if plan.uses_ray_environments() {
                return true;
            }
            todo.extend(plan.children.iter().map(|(child, _)| child.to_ascii_lowercase()));
        }
        false
    }

    pub fn status(&self, name: &str) -> EffectStatus {
        let key = name.trim().to_ascii_lowercase();
        if self.library.contains_key(&key) {
            EffectStatus::Loaded
        } else if self.missing.contains(&key) {
            EffectStatus::NotFound
        } else {
            EffectStatus::Loading
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParticleSprite {
    pub ribbon: Option<hp_render::ParticleRibbon>,
    pub animation: hp_render::ParticleAnimation,
    pub pos: [f32; 3],
    pub size: f32,
    pub axis: [f32; 3],
    pub roll: f32,
    pub yaw: f32,
    pub uv: [f32; 4],
    pub color: [f32; 4],
    pub bright: f32,
    pub add_self: f32,
    pub orient: ParticleOrient,
    pub blend: ParticleBlend,
    pub screen_fade: [f32; 2],
    pub refraction_blur: f32,
    pub no_fog: bool,
    pub material: u32,
    pub entity: Id,
    /// Authored collection/renderer pass within this effect instance.
    pub draw_order: u32,
}

pub type ParticleHandle = Arc<Mutex<ParticleShared>>;

pub fn new_handle() -> ParticleHandle {
    Arc::new(Mutex::new(ParticleShared::default()))
}

pub fn material_key(name: &str) -> String {
    let mut k = name.trim().replace('\\', "/").to_ascii_lowercase();
    if let Some(s) = k.strip_prefix("materials/") {
        k = s.to_owned();
    }
    if let Some(s) = k.strip_suffix(".vmt") {
        k = s.to_owned();
    }
    k.trim_matches('/').to_owned()
}

pub fn guess_additive(material: &str) -> bool {
    let m = material.to_ascii_lowercase();
    ["fire", "flame", "spark", "glow", "flare", "explosion", "ember", "beam", "light"].iter().any(|w| m.contains(w))
}

pub fn read_material_look(fs: &hp_fs::GameFs, material: &str) -> MaterialLook {
    let Some(m) = fs.material(material) else {
        return MaterialLook { additive: guess_additive(material), ..MaterialLook::default() };
    };
    let sprite_mode = m.shader.eq_ignore_ascii_case("sprite") && matches!(m.get_i32("$spriterendermode"), Some(3 | 5));
    let overbright = m.get_f32("$overbrightfactor").filter(|v| v.is_finite() && *v > 0.0).unwrap_or(1.0).min(16.0);
    let texture_height = m.get("$basetexture").and_then(|t| fs.texture_size(t)).map_or(1.0, |(_, h)| h as f32);
    let sheet = m
        .get("$basetexture")
        .and_then(|t| fs.load_texture_raw(t))
        .filter(|bytes| bytes.len() <= hp_pcf::MAX_FILE_SIZE)
        .and_then(|bytes| hp_pcf::sheet_from_vtf(&bytes));
    let add_self = m.get_f32("$addself").filter(|v| v.is_finite() && *v > 0.0).unwrap_or(0.0).min(16.0);
    let start = m.get_f32("$startfadesize").filter(|v| v.is_finite()).unwrap_or(10.0);
    let end = m.get_f32("$endfadesize").filter(|v| v.is_finite()).unwrap_or(20.0);
    let fade_size = if start < 4.0 && end > start { [start.max(0.0), end.min(4.0)] } else { [0.0; 2] };
    let opacity = m
        .get_f32("srgb?$alpha")
        .or_else(|| m.get_f32("$alpha"))
        .filter(|v| v.is_finite())
        .map_or(1.0, |v| v.clamp(0.0, 1.0));
    let spritecard = m.shader.eq_ignore_ascii_case("spritecard");
    let blend_frames = spritecard && m.get_i32("$blendframes").unwrap_or(1) != 0;
    MaterialLook {
        found: true,
        spritecard,
        additive: m.flag("$additive") || sprite_mode,
        overbright,
        add_self,
        fade_size,
        opacity,
        blend_frames,
        dual_sequence: spritecard && m.flag("$dualsequence"),
        sequence_mode: m.get_i32("$sequence_blend_mode").unwrap_or(0).clamp(0, 2) as u8,
        max_luminance: [spritecard && m.flag("$maxlumframeblend1"); 2],
        sheet,
        texture_height,
        refraction: m
            .shader
            .eq_ignore_ascii_case("refract")
            .then(|| m.get_f32("$refractamount").filter(|v| v.is_finite()).unwrap_or(0.02).clamp(-1.0, 1.0)),
        refraction_blur: m.get_f32("$bluramount").filter(|v| v.is_finite()).unwrap_or(0.0).clamp(0.0, 4.0),
        no_fog: m.flag("$nofog"),
    }
}

#[derive(Debug, Default)]
pub struct LoadedFile {
    pub plans: Vec<Arc<Plan>>,
    pub looks: Vec<(String, Arc<MaterialLook>)>,
}

pub fn load_file(fs: &hp_fs::GameFs, path: &str) -> Result<LoadedFile, String> {
    let data = fs.read(path).ok_or_else(|| format!("{path} not found"))?;
    if data.len() > hp_pcf::MAX_FILE_SIZE {
        return Err(format!("{path} is too large"));
    }
    let systems = hp_pcf::parse_file(&data).map_err(|e| e.to_string())?;
    let plans: Vec<Arc<Plan>> = systems.iter().map(|s| Arc::new(Plan::new(s))).collect();
    let mut looks: Vec<(String, Arc<MaterialLook>)> = Vec::new();
    for p in plans.iter().filter(|p| p.draws) {
        let key = material_key(&p.material);
        if key.is_empty() || looks.iter().any(|(k, _)| *k == key) {
            continue;
        }
        looks.push((key.clone(), Arc::new(read_material_look(fs, &key))));
    }
    Ok(LoadedFile { plans, looks })
}

pub fn manifest_files(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("");
        let tokens: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
        let [key, value, ..] = tokens.as_slice() else { continue };
        if !key.trim().eq_ignore_ascii_case("file") {
            continue;
        }
        let mut path = value.trim().trim_start_matches('!').replace('\\', "/").to_ascii_lowercase();
        while path.contains("//") {
            path = path.replace("//", "/");
        }
        let path = path.trim_matches('/').to_owned();
        if path.ends_with(".pcf") && !out.contains(&path) {
            out.push(path);
            if out.len() >= 1024 {
                break;
            }
        }
    }
    out
}

const SPRITE_COVERAGE: f32 = 0.4;

pub fn material_tint(material: &str) -> [f32; 3] {
    let m = material.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| m.contains(w));
    if has(&["fire", "flame", "burn", "ember", "lava", "explosion"]) {
        [1.0, 0.55, 0.15]
    } else if has(&["spark"]) {
        [1.0, 0.85, 0.4]
    } else if has(&["blood"]) {
        [0.6, 0.06, 0.05]
    } else if has(&["water", "splash", "rain", "drip"]) {
        [0.6, 0.75, 1.0]
    } else {
        [1.0, 1.0, 1.0]
    }
}

pub fn particle_uv(plan: &Plan, p: &hp_pcf::Particle, sheet: Option<&Sheet>) -> [f32; 4] {
    particle_uv_for(&plan.render, p, sheet)
}

fn particle_uv_for(render: &Render, p: &hp_pcf::Particle, sheet: Option<&Sheet>) -> [f32; 4] {
    let mut uv = match sheet {
        Some(sheet) => {
            let total = sheet.sequence(p.sequence).map_or(0.0, |s| s.total);
            sheet.frame_uv(p.sequence, render.sequence_cycle(p, total, false))
        }
        None => hp_pcf::sheet::FULL_UV,
    };
    if p.flip {
        uv.swap(0, 2);
    }
    uv
}

pub fn particle_uv_blend(plan: &Plan, p: &hp_pcf::Particle, sheet: &Sheet) -> ([f32; 4], [f32; 4], f32) {
    particle_uv_blend_for(&plan.render, p, sheet)
}

fn particle_uv_blend_for(render: &Render, p: &hp_pcf::Particle, sheet: &Sheet) -> ([f32; 4], [f32; 4], f32) {
    let total = sheet.sequence(p.sequence).map_or(0.0, |s| s.total);
    let (mut a, mut b, f) = sheet.frame_blend(p.sequence, render.sequence_cycle(p, total, false));
    if p.flip {
        a.swap(0, 2);
        b.swap(0, 2);
    }
    (a, b, f)
}

pub fn orientation(orient: u8) -> ParticleOrient {
    match orient {
        1 => ParticleOrient::Upright,
        2 | 3 => ParticleOrient::Flat,
        _ => ParticleOrient::Screen,
    }
}

pub fn append_models<'a>(
    effects: impl Iterator<Item = (Id, &'a Effect)>,
    models: &mut Vec<hp_app::state::ParticleModelDraw>,
) {
    // Model color modulation takes the truncated byte, then GammaToLinear(2.2).
    // ModelInstance's existing shader takes sRGB input, so encode that linear
    // value without changing the tint contract for ordinary map entities.
    let tint = |v: f32| {
        let value = f32::from((v * 255.0) as i32 as u8) / 255.0;
        let linear = value.powf(2.2);
        if linear <= 0.0031308 { linear * 12.92 } else { 1.055 * linear.powf(1.0 / 2.4) - 0.055 }
    };
    for (entity, effect) in effects {
        let mut next_draw_order = 0;
        for sim in effect.drawable_sims() {
            let plan = sim.plan();
            let particles = &sim.particles;
            let time = sim.time();
            if particles.is_empty() {
                continue;
            }
            for (renderer_index, render) in plan.renderers().enumerate() {
                let draw_order = next_draw_order;
                next_draw_order += 1;
                let Some(model) = render.model.and_then(|index| plan.model_renderers.get(index)) else { continue };
                let path: Arc<str> = model.path().into();
                let activity: Arc<str> = model.activity.as_str().into();
                for p in particles {
                    let alpha = f32::from((p.alpha * p.alpha2 * 255.0) as i32 as u8) / 255.0;
                    if alpha <= 0.0 || p.radius == 0.0 || !p.pos.iter().all(|v| v.is_finite()) {
                        continue;
                    }
                    models.push(hp_app::state::ParticleModelDraw {
                        definition: Arc::downgrade(plan),
                        renderer_index: renderer_index as u32,
                        entity,
                        draw_order,
                        model: Arc::clone(&path),
                        skin: model.skin,
                        body_group: p.sequence as i32,
                        sequence: p.sequence2 as i32,
                        activity: Arc::clone(&activity),
                        animation_time: time,
                        animation_rate: model.rate(p),
                        transform: model.transform(p),
                        tint: [tint(p.color[0]), tint(p.color[1]), tint(p.color[2]), alpha],
                    });
                }
            }
        }
    }
}

pub fn append_sprites<'a>(
    effects: impl Iterator<Item = (Id, &'a Effect)>,
    looks: &HashMap<String, Arc<MaterialLook>>,
    sprites: &mut Vec<ParticleSprite>,
    materials: &mut Vec<String>,
) {
    let mut material_of = |name: &str| -> u32 {
        match materials.iter().position(|n| n.eq_ignore_ascii_case(name)) {
            Some(i) => i as u32,
            None => {
                materials.push(name.to_owned());
                (materials.len() - 1) as u32
            }
        }
    };
    for (id, effect) in effects {
        let mut next_draw_order = 0;
        for sim in effect.drawable_sims() {
            let plan = sim.plan();
            let particles = &sim.particles;
            let dt = sim.render_dt();
            if particles.is_empty() {
                continue;
            }
            let key = material_key(&plan.material);
            let guessed;
            let look = match looks.get(&key) {
                Some(l) => &**l,
                None => {
                    guessed = MaterialLook { additive: guess_additive(&key), ..MaterialLook::default() };
                    &guessed
                }
            };
            let blend = look.blend();
            let bright = look.brightness();
            let add_self = look.blend_parameter();
            let material = material_of(&plan.material);
            for render in plan.renderers() {
                let draw_order = next_draw_order;
                next_draw_order += 1;
                let base = |p: &hp_pcf::Particle| ParticleSprite {
                    ribbon: None,
                    animation: hp_render::ParticleAnimation::default(),
                    pos: p.pos,
                    size: p.radius * 2.0,
                    axis: [0.0; 3],
                    roll: p.rotation,
                    yaw: p.yaw,
                    uv: particle_uv_for(&render, p, look.sheet.as_ref()),
                    color: [p.color[0], p.color[1], p.color[2], p.opacity()],
                    bright,
                    add_self,
                    orient: orientation(render.orient),
                    blend,
                    screen_fade: look.fade_size,
                    refraction_blur: look.refraction_blur,
                    no_fog: look.no_fog,
                    material,
                    entity: id,
                    draw_order,
                };
                let visible =
                    |p: &hp_pcf::Particle| p.opacity() > 0.0 && p.radius > 0.0 && p.pos.iter().all(|v| v.is_finite());
                match render.kind {
                    RenderKind::Sprites => {
                        for p in particles.iter().filter(|p| visible(p)) {
                            let mut s = base(p);
                            if render.orient == 3 {
                                s.axis = [0.0, 0.0, 1.0];
                            }
                            push_blended(sprites, s, &render, p, look);
                        }
                    }
                    RenderKind::Trail => {
                        let path = if look.spritecard { hp_pcf::trail::Path::Spline } else { hp_pcf::trail::Path::Cpu };
                        for p in particles.iter().filter(|p| p.pos.iter().all(|v| v.is_finite())) {
                            let Some(segment) = render.trail_segment(p, dt, path) else { continue };
                            let mut s = base(p);
                            s.pos = segment.head;
                            s.axis = segment.delta;
                            s.size = segment.width;
                            s.orient =
                                if look.spritecard { ParticleOrient::TrailSpline } else { ParticleOrient::TrailCpu };
                            s.roll = 0.0;
                            s.yaw = 0.0;
                            push_blended(sprites, s, &render, p, look);
                        }
                    }
                    RenderKind::Rope => {
                        let Some(rope) = render.rope else { continue };
                        let points = rope.points(
                            particles,
                            look.texture_height,
                            sim.time(),
                            sim.render_points(),
                            look.spritecard,
                        );
                        for (index, pair) in points.windows(2).enumerate() {
                            let [a, b] = [pair[0], pair[1]];
                            if !a.position.iter().chain(b.position.iter()).all(|v| v.is_finite()) {
                                continue;
                            }
                            let mut s = base(&particles[0]);
                            s.pos = a.position;
                            s.axis = std::array::from_fn(|i| b.position[i] - a.position[i]);
                            s.size = a.width;
                            let color = if look.spritecard {
                                points[(index / rope.subdivisions) * rope.subdivisions].color
                            } else {
                                a.color
                            };
                            s.color = color;
                            s.roll = 0.0;
                            s.yaw = 0.0;
                            s.uv = [0.0, a.v, 1.0, b.v];
                            s.orient =
                                if look.spritecard { ParticleOrient::RopeSpline } else { ParticleOrient::RopeCpu };
                            s.ribbon = Some(hp_render::ParticleRibbon {
                                neighbors: if look.spritecard {
                                    [a.tangent, b.tangent]
                                } else {
                                    [
                                        points[index.saturating_sub(1)].position,
                                        points[(index + 2).min(points.len() - 1)].position,
                                    ]
                                },
                                end_width: b.width,
                                spline: look.spritecard.then(|| {
                                    let step = index % rope.subdivisions;
                                    let increment = (rope.subdivisions as f32).recip();
                                    let mut t = 0.0;
                                    for _ in 0..step {
                                        t += increment;
                                    }
                                    hp_render::ParticleSpline {
                                        controls: hp_pcf::rope::Rope::spline_controls(
                                            particles,
                                            index / rope.subdivisions,
                                        ),
                                        interval: [t, if step + 1 == rope.subdivisions { 1.0 } else { t + increment }],
                                    }
                                }),
                                end_color: if look.spritecard
                                    && ((index + 1) % rope.subdivisions != 0 || index + 2 == points.len())
                                {
                                    color
                                } else {
                                    b.color
                                },
                            });
                            sprites.push(s);
                        }
                    }
                    RenderKind::None | RenderKind::Models => {}
                }
            }
        }
    }
}

fn push_blended(
    sprites: &mut Vec<ParticleSprite>,
    mut s: ParticleSprite,
    render: &Render,
    p: &hp_pcf::Particle,
    look: &MaterialLook,
) {
    if let Some(sheet) = &look.sheet {
        let (a, b, f) = particle_uv_blend_for(render, p, sheet);
        let total = sheet.sequence(p.sequence2).map_or(0.0, |sequence| sequence.total);
        let cycle = render.sequence_cycle(p, total, true);
        let (mut c, mut d, second) = sheet.frame_blend(p.sequence2, cycle);
        if p.flip {
            c.swap(0, 2);
            d.swap(0, 2);
        }
        s.uv = a;
        s.animation = hp_render::ParticleAnimation {
            frames: [b, c, d],
            blend: [if look.blend_frames || look.max_luminance[0] { f } else { 0.0 }, second],
            dual_sequence: look.dual_sequence,
            sequence_mode: look.sequence_mode,
            max_luminance: look.max_luminance,
        };
    }
    sprites.push(s);
}

/// Prioritize independent effects by their farthest submitted particle. Within
/// an effect, preserve collection/renderer passes and sort each pass separately.
pub fn sort_far_first(sprites: &mut [ParticleSprite], eye: [f32; 3]) {
    let d2 = |s: &ParticleSprite| {
        let d = [s.pos[0] - eye[0], s.pos[1] - eye[1], s.pos[2] - eye[2]];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
    };
    let mut effect_distance = HashMap::<Id, f32>::new();
    for sprite in sprites.iter() {
        let distance = d2(sprite);
        effect_distance.entry(sprite.entity).and_modify(|d| *d = d.max(distance)).or_insert(distance);
    }
    sprites.sort_by(|a, b| {
        if a.entity == b.entity {
            a.draw_order.cmp(&b.draw_order).then_with(|| {
                if a.ribbon.is_some() && b.ribbon.is_some() {
                    std::cmp::Ordering::Equal
                } else {
                    d2(b).total_cmp(&d2(a))
                }
            })
        } else {
            effect_distance[&b.entity].total_cmp(&effect_distance[&a.entity]).then_with(|| a.entity.cmp(&b.entity))
        }
    });
}

/// Merge editor entity effects with PCFs before losing their collection/pass
/// keys. Sorting the GPU instances afterward would destroy child draw order.
pub fn scene_instances_of(
    shared: &ParticleShared,
    eye: [f32; 3],
    assets: &mut dyn hp_app::state::AssetsApi,
) -> Vec<hp_render::ParticleInstance> {
    let (sprites, materials) = merged_scene_sprites(shared, eye);
    instances_of(&sprites, &materials, assets)
}

fn merged_scene_sprites(
    shared: &ParticleShared,
    eye: [f32; 3],
) -> (std::borrow::Cow<'_, [ParticleSprite]>, std::borrow::Cow<'_, [String]>) {
    if shared.fx_sprites.is_empty() {
        return (std::borrow::Cow::Borrowed(&shared.sprites), std::borrow::Cow::Borrowed(&shared.materials));
    }
    let mut sprites = Vec::with_capacity(shared.sprites.len() + shared.fx_sprites.len());
    sprites.extend_from_slice(&shared.sprites);
    let offset = shared.materials.len() as u32;
    sprites.extend(
        shared.fx_sprites.iter().map(|sprite| ParticleSprite { material: sprite.material + offset, ..*sprite }),
    );
    let mut materials = shared.materials.clone();
    materials.extend_from_slice(&shared.fx_materials);
    sort_far_first(&mut sprites, eye);
    (std::borrow::Cow::Owned(sprites), std::borrow::Cow::Owned(materials))
}

#[derive(Default)]
pub struct ParticleDraws {
    pub sprites: Vec<hp_render::ParticleInstance>,
    pub models: Vec<hp_render::ModelInstance>,
    pub batches: Vec<hp_render::ParticleDrawBatch>,
}

pub fn scene_draws_of(
    shared: &ParticleShared,
    eye: [f32; 3],
    assets: &mut dyn hp_app::state::AssetsApi,
    renderer: &mut hp_render::Renderer,
) -> ParticleDraws {
    let (sprites, materials) = merged_scene_sprites(shared, eye);
    prepare_draws(&sprites, &materials, &shared.model_draws, eye, assets, renderer)
}

/// Resolve each collection/pass before erasing its authored key. Missing or
/// still-loading models contribute zero instances rather than shifting ranges.
pub fn prepare_draws(
    sprites: &[ParticleSprite],
    materials: &[String],
    models: &[hp_app::state::ParticleModelDraw],
    eye: [f32; 3],
    assets: &mut dyn hp_app::state::AssetsApi,
    renderer: &mut hp_render::Renderer,
) -> ParticleDraws {
    if models.is_empty() {
        return ParticleDraws { sprites: instances_of(sprites, materials, assets), ..Default::default() };
    }
    #[derive(Default)]
    struct Group {
        sprites: Vec<ParticleSprite>,
        models: Vec<hp_app::state::ParticleModelDraw>,
    }
    let distance = |p: [f32; 3]| p.iter().zip(eye).map(|(p, eye)| (p - eye) * (p - eye)).sum::<f32>();
    let mut groups = std::collections::BTreeMap::<(Id, u32), Group>::new();
    let mut distances = HashMap::<Id, f32>::new();
    let mut note = |entity: Id, pos: [f32; 3]| {
        let d = distance(pos);
        distances.entry(entity).and_modify(|old| *old = old.max(d)).or_insert(d);
    };
    for sprite in sprites.iter().filter(|p| p.pos.iter().all(|v| v.is_finite())) {
        note(sprite.entity, sprite.pos);
        groups.entry((sprite.entity, sprite.draw_order)).or_default().sprites.push(*sprite);
    }
    for model in models {
        let p = model.transform[3];
        note(model.entity, [p[0], p[1], p[2]]);
        groups.entry((model.entity, model.draw_order)).or_default().models.push(model.clone());
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|(a, _), (b, _)| {
        if a.0 == b.0 { a.1.cmp(&b.1) } else { distances[&b.0].total_cmp(&distances[&a.0]).then_with(|| a.0.cmp(&b.0)) }
    });
    let mut out = ParticleDraws::default();
    for (_, group) in groups {
        let first = out.sprites.len() as u32;
        out.sprites.extend(instances_of(&group.sprites, materials, assets));
        if out.sprites.len() as u32 > first {
            out.batches.push(hp_render::ParticleDrawBatch {
                kind: hp_render::ParticleDrawKind::Sprites,
                first,
                count: out.sprites.len() as u32 - first,
            });
        }
        let first = out.models.len() as u32;
        out.models.extend(assets.particle_draw_models(renderer, &group.models).into_iter().filter(|model| {
            renderer.has_model(model.model)
                && model.transform.iter().flatten().chain(&model.tint).all(|v| v.is_finite())
                && model.tint[3] > 0.0
        }));
        if out.models.len() as u32 > first {
            out.batches.push(hp_render::ParticleDrawBatch {
                kind: hp_render::ParticleDrawKind::Models,
                first,
                count: out.models.len() as u32 - first,
            });
        }
    }
    if out.models.is_empty() {
        out.batches.clear();
    }
    out
}

pub fn instances_of(
    sprites: &[ParticleSprite],
    materials: &[String],
    assets: &mut dyn hp_app::state::AssetsApi,
) -> Vec<hp_render::ParticleInstance> {
    let mut ids: Vec<[Option<hp_render::MaterialId>; 2]> = vec![[None; 2]; materials.len()];
    sprites
        .iter()
        .filter_map(|p| {
            let i = p.material as usize;
            let additive = p.blend == ParticleBlend::Additive;
            let slot = &mut ids.get_mut(i)?[usize::from(additive)];
            let material = if p.blend == ParticleBlend::Refract {
                assets.refractive_sprite_material_id(&materials[i])
            } else {
                *slot.get_or_insert_with(|| assets.sprite_material_id(&materials[i], additive))
            };
            Some(hp_render::ParticleInstance {
                ribbon: p.ribbon,
                animation: p.animation,
                pos: p.pos,
                size: p.size,
                axis: p.axis,
                roll: p.roll,
                yaw: p.yaw,
                uv: p.uv,
                color: p.color,
                bright: p.bright,
                add_self: p.add_self,
                orient: p.orient,
                material,
                screen_fade: p.screen_fade,
                refraction_blur: p.refraction_blur,
                no_fog: p.no_fog,
                blend: p.blend,
            })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
struct Source {
    id: Id,
    name: String,
    label: String,
    origin: DVec3,
    points: Points,
    model_objects: Arc<HashSet<Id>>,
}

#[derive(Clone, Debug)]
struct LightingContext {
    baked: Option<Arc<hp_editor::limits::world_light::PropLighting>>,
    region: Option<[[f64; 3]; 2]>,
    compiled: Option<Arc<hp_editor::limits::world::BspWorld>>,
}

impl LightingContext {
    fn new(state: &AppState, lighting: &hp_app::state::ParticleLighting) -> Self {
        let compiled = if state.runtime.lab.player.active && !state.runtime.lab.player.editor_world {
            state.runtime.health.world.clone()
        } else {
            None
        };
        Self {
            baked: if compiled.is_none() { lighting.baked.clone() } else { None },
            region: if compiled.is_none() { lighting.region } else { None },
            compiled,
        }
    }
}

impl PartialEq for LightingContext {
    fn eq(&self, other: &Self) -> bool {
        self.baked.as_ref().map(Arc::as_ptr) == other.baked.as_ref().map(Arc::as_ptr)
            && self.compiled.as_ref().map(Arc::as_ptr) == other.compiled.as_ref().map(Arc::as_ptr)
            && self.region == other.region
    }
}

struct Playing {
    name: String,
    effect: Effect,
    rebuild_after: Option<u64>,
}

#[derive(Clone, Debug, PartialEq)]
enum SourcesKey {
    Document(u64, u64),
    Compiled(usize),
}

pub struct ParticleLayer {
    handle: ParticleHandle,
    sources: Vec<Source>,
    sources_key: Option<SourcesKey>,
    effects: HashMap<Id, Playing>,
    lighting: Option<LightingContext>,
}

fn parse3(s: Option<&str>) -> Option<[f32; 3]> {
    let v: Vec<f32> = s?.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.len() == 3 && v.iter().all(|x| x.is_finite())).then(|| [v[0], v[1], v[2]])
}

fn f3(v: DVec3) -> [f32; 3] {
    [v.x as f32, v.y as f32, v.z as f32]
}

pub(crate) fn compiled_entity_id(index: usize, entity: &hp_vmf::Entity) -> Id {
    entity.get("hammerid").and_then(|value| value.trim().parse().ok()).unwrap_or(BSP_ID_BASE + index as Id)
}

fn sources_of<'a>(entities: impl Iterator<Item = (Id, &'a hp_vmf::Entity)> + Clone) -> Vec<Source> {
    let model_objects: Arc<HashSet<Id>> = Arc::new(
        entities
            .clone()
            .filter(|(_, entity)| entity.get("model").is_some_and(|path| path.to_ascii_lowercase().ends_with(".mdl")))
            .map(|(id, _)| id)
            .collect(),
    );
    let is_system = |e: &hp_vmf::Entity| e.classname.eq_ignore_ascii_case("info_particle_system");
    let wants_points = entities.clone().any(|(_, e)| {
        is_system(e)
            && e.props.iter().any(|(k, v)| k.to_ascii_lowercase().starts_with("cpoint") && !v.trim().is_empty())
    });
    let mut named: HashMap<String, (Id, DVec3, [f32; 3])> = HashMap::new();
    if wants_points {
        for (id, e) in entities.clone() {
            if let (Some(n), Some(o)) = (e.get("targetname"), e.origin()) {
                let n = n.trim().to_ascii_lowercase();
                if !n.is_empty() && o.is_finite() {
                    named.entry(n).or_insert((id, o, parse3(e.get("angles")).unwrap_or([0.0; 3])));
                }
            }
        }
    }
    let mut out = Vec::new();
    for (id, e) in entities {
        if !is_system(e) {
            continue;
        }
        let Some(name) = e.get("effect_name").map(str::trim).filter(|n| !n.is_empty()) else { continue };
        let Some(origin) = e.origin().filter(|o| o.is_finite()) else { continue };
        let angles = parse3(e.get("angles")).unwrap_or([0.0; 3]);
        let mut points = Points::with_angles(f3(origin), angles);
        points.objects.push(Some(id));
        for k in 1..hp_pcf::sim::MAX_CONTROL_POINTS {
            if let Some(parent) = e.get(&format!("cpoint{k}_parent")).and_then(|value| value.trim().parse::<i32>().ok())
            {
                points.parents.resize(k + 1, 0);
                points.parents[k] = parent.clamp(0, 63);
            }
        }
        if !named.is_empty() {
            for k in 1..hp_pcf::sim::MAX_CONTROL_POINTS {
                let Some(target) = e.get(&format!("cpoint{k}")).map(|t| t.trim().to_ascii_lowercase()) else {
                    continue;
                };
                let Some((object, p, angles)) = named.get(&target) else { continue };
                if points.pos.len() <= k {
                    points.pos.resize(k + 1, [0.0; 3]);
                }
                points.pos[k] = f3(*p);
                if points.objects.len() <= k {
                    points.objects.resize(k + 1, None);
                }
                points.objects[k] = Some(*object);
                if points.orientations.len() <= k {
                    points.orientations.resize(k + 1, Points::UNSET_AXES);
                }
                points.orientations[k] = Points::with_angles(f3(*p), *angles).axes;
            }
        }
        out.push(Source {
            id,
            name: name.to_ascii_lowercase(),
            label: name.to_owned(),
            origin,
            points,
            model_objects: Arc::clone(&model_objects),
        });
    }
    out
}

impl ParticleLayer {
    pub fn new(handle: ParticleHandle) -> Self {
        Self { handle, sources: Vec::new(), sources_key: None, effects: HashMap::new(), lighting: None }
    }

    pub fn particle_count(&self) -> usize {
        self.effects.values().map(|p| p.effect.particle_count()).sum()
    }

    fn compiled_world(state: &AppState) -> Option<&Arc<hp_editor::limits::world::BspWorld>> {
        let p = &state.runtime.lab.player;
        if !p.active || p.editor_world {
            return None;
        }
        state.runtime.health.world.as_ref()
    }

    fn refresh_sources(&mut self, state: &AppState) {
        let key = match Self::compiled_world(state) {
            Some(w) => SourcesKey::Compiled(Arc::as_ptr(w) as usize),
            None => SourcesKey::Document(state.sidecar.serial(), state.doc.revision()),
        };
        if self.sources_key.as_ref() == Some(&key) {
            return;
        }
        self.sources_key = Some(key);
        self.sources = match Self::compiled_world(state) {
            Some(w) => {
                let list: Vec<(Id, &hp_vmf::Entity)> =
                    w.particle_entities.iter().enumerate().map(|(i, e)| (compiled_entity_id(i, e), e)).collect();
                sources_of(list.iter().map(|(id, e)| (*id, *e)))
            }
            None => sources_of(state.doc.scene().entities().iter().map(|(id, node)| (*id, &node.entity))),
        };
    }

    #[cfg(test)]
    fn start(shared: &mut ParticleShared, s: &Source) -> Option<Playing> {
        Self::start_in(shared, s, &hp_pcf::environment::EmptyEnvironment)
    }

    fn start_in(
        shared: &mut ParticleShared,
        s: &Source,
        world: &dyn hp_pcf::environment::Environment,
    ) -> Option<Playing> {
        let library = &shared.library;
        let lookup = |n: &str| library.get(&n.to_ascii_lowercase()).cloned();
        let Some(mut effect) = Effect::new(&s.name, &lookup, s.id) else {
            if !shared.missing.contains(&s.name) {
                shared.wanted.insert(s.name.clone());
            }
            return None;
        };
        let mut rebuild_after = None;
        for child in &effect.missing {
            let child = child.to_ascii_lowercase();
            if !shared.missing.contains(&child) {
                shared.wanted.insert(child);
                rebuild_after = Some(shared.library_gen);
            }
        }
        let looks = &shared.looks;
        effect.set_sequence_frames(&|plan: &Plan| {
            let sheet = looks.get(&material_key(&plan.material))?.sheet.as_ref()?;
            let n = sheet.sequences.iter().map(|q| q.number).max()?.min(63) as usize;
            Some((0..=n as u32).map(|i| sheet.frame_count(i) as u32).collect())
        });
        if effect.continuous() {
            let steps = (WARM_UP / 0.05) as usize;
            for _ in 0..steps {
                effect.step_in(0.05, &s.points, MAX_PER_EFFECT, world);
            }
        }
        Some(Playing { name: s.name.clone(), effect, rebuild_after })
    }

    fn fill_sprites(&self, shared: &mut ParticleShared, eye: [f32; 3]) {
        let ParticleShared { sprites, materials, model_draws, looks, .. } = shared;
        sprites.clear();
        materials.clear();
        append_sprites(self.effects.iter().map(|(id, p)| (*id, &p.effect)), looks, sprites, materials);
        model_draws.clear();
        append_models(self.effects.iter().map(|(id, p)| (*id, &p.effect)), model_draws);
        sort_far_first(sprites, eye);
    }

    fn stop_all(&mut self) {
        self.effects.clear();
        self.lighting = None;
        if let Ok(mut s) = self.handle.lock() {
            s.world_requirement = None;
            s.model_requirement.clear();
            s.ray_requirement = false;
            s.active = false;
            s.playing.clear();
            s.particles = 0;
            s.sprites.clear();
            s.model_draws.clear();
        }
    }
}

impl OverlayLayer for ParticleLayer {
    fn name(&self) -> &'static str {
        "lab_particles"
    }

    fn prepare(&mut self, state: &AppState) {
        let settings = &state.settings.lab;
        if !settings.show_particles {
            if let Ok(mut shared) = self.handle.lock() {
                shared.world_requirement = None;
                shared.model_requirement.clear();
                shared.ray_requirement = false;
            }
            if !self.effects.is_empty() || self.handle.lock().is_ok_and(|s| s.active || !s.sprites.is_empty()) {
                self.stop_all();
            }
            return;
        }
        self.refresh_sources(state);
        let compiled = Self::compiled_world(state).is_some();
        let cam = state.cameras.cam3d;
        let forward = cam.forward();
        let mut near: Vec<(f64, &Source)> = self
            .sources
            .iter()
            .filter(|s| !settings.particles_selected_only || state.doc.is_selected(ObjRef::Entity(s.id)))
            .filter(|s| compiled || state.doc.is_entity_visible(s.id))
            .filter_map(|s| {
                let d = s.origin - cam.pos;
                let dist = d.length();
                (dist <= MAX_DISTANCE && (dist < 256.0 || d.dot(forward) > -0.2 * dist)).then_some((dist, s))
            })
            .collect();
        near.sort_by(|a, b| a.0.total_cmp(&b.0));
        near.truncate(MAX_EFFECTS);
        let Ok(mut shared) = self.handle.lock() else { return };
        let requirements: HashMap<_, _> = near
            .iter()
            .map(|(_, source)| {
                let inside = shared.model_control_points(&source.name, true).into_iter().any(|cp| {
                    source
                        .points
                        .objects
                        .get(cp as usize)
                        .copied()
                        .flatten()
                        .is_some_and(|id| source.model_objects.contains(&id))
                });
                (source.id, if inside { Some(true) } else { shared.trace_requirement(&source.name) })
            })
            .collect();
        shared.world_requirement = requirements.values().copied().flatten().max();
        let model_requirements: HashMap<_, Vec<_>> = near
            .iter()
            .map(|(_, source)| {
                let objects = shared
                    .model_control_points(&source.name, false)
                    .into_iter()
                    .filter_map(|cp| source.points.objects.get(cp as usize).copied().flatten())
                    .filter(|id| source.model_objects.contains(id))
                    .collect();
                (source.id, objects)
            })
            .collect();
        shared.model_requirement = model_requirements.values().flatten().copied().collect();
        let models = Arc::clone(&shared.models);
        let ray_requirements: HashSet<_> = near
            .iter()
            .filter(|(_, source)| shared.uses_ray_environments(&source.name))
            .map(|(_, source)| source.id)
            .collect();
        shared.ray_requirement = !ray_requirements.is_empty();
        let rays = Arc::clone(&shared.rays);
        let physics = Arc::clone(&shared.physics);
        let mut lighting = state.assets.particle_lighting();
        if !state.runtime.lighting.has_bake {
            lighting.baked = None;
            lighting.region = None;
        }
        let context = LightingContext::new(state, &lighting);
        if self.lighting.as_ref() != Some(&context) {
            for playing in self.effects.values_mut() {
                playing.effect.invalidate_lighting();
            }
            self.lighting = Some(context);
        }
        let world = crate::particle_world::ParticleEnvironment {
            state,
            physics: &physics,
            lighting: Some(&lighting),
            rays: Some(&rays),
            models: Some(&models),
        };
        let keep: HashSet<Id> = near.iter().map(|(_, s)| s.id).collect();
        self.effects.retain(|id, _| keep.contains(id));
        let dt = state.frame.dt.clamp(0.0, 0.1);
        let particle_view = hp_pcf::screen_velocity::view(f3(cam.pos), f3(cam.right()), f3(cam.up()));
        let mut budget = MAX_PARTICLES;
        let mut starts = 0;
        for (_, s) in &near {
            if model_requirements.get(&s.id).is_some_and(|objects| !objects.is_empty() && !models.ready(state, objects))
            {
                continue;
            }
            if ray_requirements.contains(&s.id) && !rays.ready(state) {
                continue;
            }
            if requirements.get(&s.id).copied().flatten().is_some_and(|props| !physics.ready(state, props)) {
                continue;
            }
            let stale = self
                .effects
                .get(&s.id)
                .is_none_or(|p| p.name != s.name || p.rebuild_after.is_some_and(|g| shared.library_gen != g));
            if stale {
                if starts >= STARTS_PER_FRAME && !self.effects.contains_key(&s.id) {
                    continue;
                }
                starts += 1;
                match Self::start_in(&mut shared, s, &world) {
                    Some(p) => {
                        self.effects.insert(s.id, p);
                    }
                    None => {
                        self.effects.remove(&s.id);
                        continue;
                    }
                }
            }
            if let Some(p) = self.effects.get_mut(&s.id) {
                p.effect.step_in(dt, &s.points, budget.min(MAX_PER_EFFECT), &world);
                p.effect.prepare_render(particle_view);
                budget = budget.saturating_sub(p.effect.particle_count());
            }
        }
        shared.particles = self.particle_count();
        shared.active = shared.particles > 0;
        shared.playing = self.effects.iter().map(|(id, p)| (*id, p.name.clone())).collect();
        shared.playing.sort();
        if shared.sprite_layer {
            self.fill_sprites(&mut shared, f3(cam.pos));
        } else {
            shared.sprites.clear();
        }
    }

    fn add(&self, _state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if view.kind.is_2d() || self.effects.is_empty() {
            return;
        }
        if self.handle.lock().is_ok_and(|s| s.sprite_layer) {
            return;
        }
        let right = view.cam3d.right();
        let up = view.cam3d.up();
        let (r, u) = (f3(right), f3(up));
        const HEX: [(f32, f32); 6] =
            [(1.0, 0.0), (0.5, 0.866), (-0.5, 0.866), (-1.0, 0.0), (-0.5, -0.866), (0.5, -0.866)];
        for playing in self.effects.values() {
            let mut last: Option<(*const Plan, [f32; 3])> = None;
            for (plan, p) in playing.effect.particles() {
                if p.opacity() <= 0.01 || p.radius <= 0.0 {
                    continue;
                }
                let tint = match last {
                    Some((ptr, t)) if std::ptr::eq(ptr, plan) => t,
                    _ => {
                        let t = material_tint(&plan.material);
                        last = Some((plan as *const Plan, t));
                        t
                    }
                };
                let color = [
                    p.color[0] * tint[0],
                    p.color[1] * tint[1],
                    p.color[2] * tint[2],
                    (p.opacity() * SPRITE_COVERAGE).clamp(0.0, 1.0),
                ];
                let corner = |k: usize| {
                    let (a, b) = HEX[k];
                    let s = p.radius;
                    [
                        p.pos[0] + (r[0] * a + u[0] * b) * s,
                        p.pos[1] + (r[1] * a + u[1] * b) * s,
                        p.pos[2] + (r[2] * a + u[2] * b) * s,
                    ]
                };
                let c: [[f32; 3]; 6] = std::array::from_fn(corner);
                for (a, b, d) in [(0, 1, 2), (0, 2, 3), (0, 3, 4), (0, 4, 5)] {
                    out.triangles.push(OverlayTri { p: [c[a], c[b], c[d]], color });
                }
            }
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let player = state.runtime.lab.player.active;
        if view.kind.is_2d() || player || !state.settings.lab.show_particles || self.sources.is_empty() {
            return;
        }
        let hover = painter.ctx().input(|i| i.pointer.hover_pos()).filter(|p| view.rect.contains(*p));
        let Ok(shared) = self.handle.lock() else { return };
        let mut drawn = 0;
        for s in &self.sources {
            let selected = state.doc.is_selected(ObjRef::Entity(s.id));
            let Some(at) = view.world_to_screen(s.origin) else { continue };
            if !view.rect.contains(at) {
                continue;
            }
            let hovered = hover.is_some_and(|h| h.distance(at) <= HOVER_PX);
            if !selected && !hovered {
                continue;
            }
            let text = match shared.status(&s.name) {
                EffectStatus::Loaded if self.effects.contains_key(&s.id) => s.label.clone(),
                EffectStatus::Loaded => hp_app::tf!("lab.particles.label_idle", s.label),
                EffectStatus::Loading => hp_app::tf!("lab.particles.label_loading", s.label),
                EffectStatus::NotFound => hp_app::tf!("lab.particles.label_missing", s.label),
            };
            let color = if shared.status(&s.name) == EffectStatus::Loaded {
                hp_ui_viewports::paint::TEXT
            } else {
                hp_ui_viewports::paint::TEXT_WEAK
            };
            hp_ui_viewports::paint::text_pill(
                painter,
                at + egui::vec2(0.0, -12.0),
                egui::Align2::CENTER_BOTTOM,
                text,
                color,
            );
            drawn += 1;
            if drawn >= 32 {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(name: &str) -> Arc<Plan> {
        let def = hp_pcf::SystemDef {
            name: name.to_owned(),
            material: "particle/fire".to_owned(),
            emitters: vec![hp_pcf::Operator { function: "emit_continuously".into(), ..Default::default() }],
            renderers: vec![hp_pcf::Operator { function: "render_animated_sprites".into(), ..Default::default() }],
            ..Default::default()
        };
        Arc::new(Plan::new(&def))
    }

    fn view() -> ViewInfo {
        ViewInfo {
            index: 0,
            kind: hp_app::ViewKind::Perspective,
            rect: egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
            cam3d: hp_app::view::Camera3d::default(),
            cam2d: hp_app::view::Camera2d { center: glam::DVec2::ZERO, zoom: 1.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    fn state_with_particles(n: usize) -> AppState {
        let mut state = hp_app::testing::test_state();
        let mut map = hp_vmf::Map::new_empty();
        for i in 0..n {
            let mut e =
                hp_vmf::Entity { id: 100 + i as u32, classname: "info_particle_system".into(), ..Default::default() };
            e.set("effect_name", if i % 2 == 0 { "Fire_Small" } else { "unknown_fx" });
            e.set("origin", format!("{} 0 0", i * 16));
            map.entities.push(e);
        }
        state.doc = hp_editor::Document::from_map(map);
        state.cameras.cam3d.pos = DVec3::new(-200.0, 0.0, 0.0);
        state.cameras.cam3d.yaw_deg = 0.0;
        state.cameras.cam3d.pitch_deg = 0.0;
        state.frame.dt = 1.0 / 30.0;
        state
    }

    #[test]
    fn screen_velocity_updates_the_viewport_rotation_when_the_camera_turns() {
        let definitions =
            hp_pcf::parse_file(include_bytes!("../../hp-pcf/tests/data/native_screen_velocity.pcf")).unwrap();
        for (name, draws) in [("after", true), ("before", true), ("only", false)] {
            let mut definition =
                definitions.iter().find(|d| d.name == format!("hp_velocity117_{name}")).unwrap().clone();
            definition.name = "fire_small".into();
            let handle = new_handle();
            {
                let mut shared = handle.lock().unwrap();
                shared.library.insert("fire_small".into(), Arc::new(Plan::new(&definition)));
                shared.sprite_layer = true;
            }
            let mut layer = ParticleLayer::new(handle.clone());
            let mut state = state_with_particles(1);
            state.cameras.cam3d.pos = DVec3::new(0.0, -128.0, 64.0);
            state.cameras.cam3d.yaw_deg = 90.0;
            layer.prepare(&state);
            let shared = handle.lock().unwrap();
            assert_eq!(shared.sprites.len(), usize::from(draws), "{name}");
            if draws {
                assert!((shared.sprites[0].roll - std::f32::consts::FRAC_PI_4).abs() < 0.0001, "{name}");
            }
            drop(shared);
            state.frame.dt = 0.0;
            state.cameras.cam3d.pos.y = 128.0;
            state.cameras.cam3d.yaw_deg = 270.0;
            layer.prepare(&state);
            let shared = handle.lock().unwrap();
            assert_eq!(shared.sprites.len(), usize::from(draws), "{name}");
            if draws {
                assert!((shared.sprites[0].roll - std::f32::consts::FRAC_PI_4 * 3.0).abs() < 0.0001, "{name}");
            }
        }
    }

    #[test]
    fn dense_native_parent_and_child_systems_keep_their_full_particle_counts() {
        let definition = hp_pcf::parse_file(include_bytes!("../../hp-pcf/tests/data/cap_cases.pcf"))
            .unwrap()
            .into_iter()
            .find(|system| system.name == "hp_spritecard_cap5000")
            .unwrap();
        let mut parent = definition.clone();
        parent.name = "fire_small".into();
        parent.children.push(hp_pcf::Child { system: "dense_child".into(), delay: 0.0 });
        let mut child = definition;
        child.name = "dense_child".into();
        let handle = new_handle();
        {
            let mut shared = handle.lock().unwrap();
            shared.library.insert(parent.name.clone(), Arc::new(Plan::new(&parent)));
            shared.library.insert(child.name.clone(), Arc::new(Plan::new(&child)));
            shared.sprite_layer = true;
        }
        let mut layer = ParticleLayer::new(handle.clone());
        layer.prepare(&state_with_particles(1));
        assert_eq!(layer.particle_count(), 10000);
        assert_eq!(handle.lock().unwrap().sprites.len(), 10000);
    }

    #[test]
    fn lighting_context_tracks_bake_replacements_without_resetting_for_animated_styles() {
        let mut state = hp_app::testing::test_state();
        let mut lighting = hp_app::state::ParticleLighting::default();
        let empty = LightingContext::new(&state, &lighting);
        lighting.baked = Some(Arc::new(hp_editor::limits::world_light::PropLighting::default()));
        let first = LightingContext::new(&state, &lighting);
        assert_ne!(empty, first);
        assert_eq!(first, LightingContext::new(&state, &lighting.clone()));
        lighting.styles = vec![0.5];
        assert_eq!(first, LightingContext::new(&state, &lighting));
        lighting.region = Some([[-16.0; 3], [16.0; 3]]);
        assert_ne!(first, LightingContext::new(&state, &lighting));
        lighting.region = None;
        lighting.baked = Some(Arc::new(hp_editor::limits::world_light::PropLighting::default()));
        assert_ne!(first, LightingContext::new(&state, &lighting));
        state.runtime.lab.player.active = true;
        state.runtime.lab.player.editor_world = false;
        state.runtime.health.world = Some(Arc::new(hp_editor::limits::world::BspWorld::default()));
        assert_ne!(first, LightingContext::new(&state, &lighting));
    }

    #[test]
    fn particles_are_on_by_default() {
        assert!(hp_app::settings::LabSettings::default().show_particles);
    }

    #[test]
    fn effects_play_near_the_camera_within_the_caps() {
        let handle = new_handle();
        let mut layer = ParticleLayer::new(handle.clone());
        let mut state = state_with_particles(100);
        state.settings.lab.show_particles = false;
        layer.prepare(&state);
        assert!(handle.lock().expect("lock").wanted.is_empty());
        state.settings.lab.show_particles = true;
        layer.prepare(&state);
        {
            let mut s = handle.lock().expect("lock");
            assert_eq!(s.wanted.len(), 2);
            assert_eq!(s.status("Fire_Small"), EffectStatus::Loading);
            s.wanted.clear();
            s.library.insert("fire_small".into(), plan("Fire_Small"));
            s.library_gen += 1;
            s.missing.insert("unknown_fx".into());
            assert_eq!(s.status("fire_small"), EffectStatus::Loaded);
            assert_eq!(s.status("UNKNOWN_FX"), EffectStatus::NotFound);
        }
        for _ in 0..60 {
            layer.prepare(&state);
        }
        assert!(handle.lock().expect("lock").wanted.is_empty(), "missing names are not asked again");
        assert!(layer.effects.len() <= MAX_EFFECTS);
        assert!(layer.particle_count() > 0);
        assert!(layer.particle_count() <= MAX_PARTICLES);
        {
            let s = handle.lock().expect("lock");
            assert!(s.active);
            assert_eq!(s.particles, layer.particle_count());
            assert!(!s.playing.is_empty() && s.playing.iter().all(|(_, n)| n == "fire_small"));
        }
        let mut out = Overlay::default();
        layer.add(&state, &view(), &mut out);
        assert!(!out.triangles.is_empty());
        assert!(out.triangles.len() <= MAX_PARTICLES * 4);
        let mut flat = Overlay::default();
        layer.add(&state, &ViewInfo { kind: hp_app::ViewKind::Top, ..view() }, &mut flat);
        assert!(flat.is_empty());
        handle.lock().expect("lock").sprite_layer = true;
        layer.prepare(&state);
        {
            let s = handle.lock().expect("lock");
            assert!(!s.sprites.is_empty() && s.sprites.len() <= MAX_PARTICLES);
            assert!(s.sprites.iter().all(|p| (p.material as usize) < s.materials.len() && p.size > 0.0));
            assert!(s.sprites.iter().all(|p| p.entity >= 100));
            assert!(s.sprites.iter().all(|p| p.blend == ParticleBlend::Additive));
            let eye = [-200.0f32, 0.0, 0.0];
            let d = |p: &ParticleSprite| (p.pos[0] - eye[0]).powi(2) + p.pos[1].powi(2) + p.pos[2].powi(2);
            for pass in s.sprites.chunk_by(|a, b| a.entity == b.entity && a.draw_order == b.draw_order) {
                assert!(pass.windows(2).all(|w| d(&w[0]) >= d(&w[1])), "far to near within each draw pass");
            }
        }
        let mut none = Overlay::default();
        layer.add(&state, &view(), &mut none);
        assert!(none.is_empty());
        handle.lock().expect("lock").sprite_layer = false;
        state.settings.lab.particles_selected_only = true;
        layer.prepare(&state);
        assert_eq!(layer.particle_count(), 0);
        assert!(!handle.lock().expect("lock").active);
        state.settings.lab.particles_selected_only = false;
        state.settings.lab.show_particles = false;
        layer.prepare(&state);
        assert!(layer.effects.is_empty());
        assert!(handle.lock().expect("lock").playing.is_empty());
    }

    #[test]
    fn children_found_later_rebuild_the_effect() {
        let handle = new_handle();
        let mut layer = ParticleLayer::new(handle.clone());
        let state = state_with_particles(1);
        let parent = {
            let def = hp_pcf::SystemDef {
                name: "fire_small".into(),
                emitters: vec![hp_pcf::Operator { function: "emit_continuously".into(), ..Default::default() }],
                renderers: vec![hp_pcf::Operator { function: "render_animated_sprites".into(), ..Default::default() }],
                children: vec![hp_pcf::Child { system: "Embers".into(), delay: 0.0 }],
                ..Default::default()
            };
            Arc::new(Plan::new(&def))
        };
        handle.lock().expect("lock").library.insert("fire_small".into(), parent);
        layer.prepare(&state);
        assert!(handle.lock().expect("lock").wanted.contains("embers"), "the child is asked for");
        assert_eq!(layer.effects[&100].effect.sims.len(), 1);
        {
            let mut s = handle.lock().expect("lock");
            s.wanted.clear();
            s.library.insert("embers".into(), plan("Embers"));
            s.library_gen += 1;
        }
        layer.prepare(&state);
        assert_eq!(layer.effects[&100].effect.sims.len(), 2, "rebuilt with its child");
    }

    #[test]
    fn control_points_and_angles_come_from_the_entities() {
        let mut map = hp_vmf::Map::new_empty();
        let mut sys = hp_vmf::Entity { id: 5, classname: "info_particle_system".into(), ..Default::default() };
        sys.set("effect_name", " Beam_FX ");
        sys.set("origin", "10 20 30");
        sys.set("angles", "0 90 0");
        sys.set("cpoint1", "Target");
        sys.set("cpoint1_parent", "2");
        sys.set("cpoint3", "Target");
        sys.set("cpoint4", "nobody");
        let mut target = hp_vmf::Entity { id: 6, classname: "info_target".into(), ..Default::default() };
        target.set("targetname", "target");
        target.set("origin", "100 0 50");
        map.entities.push(sys);
        map.entities.push(target);
        let doc = hp_editor::Document::from_map(map);
        let sources = sources_of(doc.scene().entities().iter().map(|(id, n)| (*id, &n.entity)));
        assert_eq!(sources.len(), 1);
        let s = &sources[0];
        assert_eq!(s.name, "beam_fx");
        assert_eq!(s.points.get(1), [100.0, 0.0, 50.0]);
        assert_eq!(s.points.parents[1], 2);
        assert_eq!(s.points.objects, vec![Some(5), Some(6), None, Some(6)]);
        assert_eq!(s.points.get(2), [0.0; 3]);
        assert_eq!(s.points.get(3), [100.0, 0.0, 50.0]);
        assert_eq!(s.points.get(4), [0.0; 3]);
        assert_eq!(s.points.orientation(1), Points::with_angles([100.0, 0.0, 50.0], [0.0; 3]).axes);
        assert_eq!(s.points.orientation(2), Points::UNSET_AXES);
        assert_eq!(s.points.orientation(3), s.points.orientation(1));
        assert_eq!(s.points.orientation(4), Points::UNSET_AXES);
        assert!((s.points.axes[0][1] - 1.0).abs() < 1e-5);
    }

    fn painted(layer: &ParticleLayer, state: &AppState, view: &ViewInfo, hover: Option<egui::Pos2>) -> usize {
        let ctx = egui::Context::default();
        let mut events = Vec::new();
        if let Some(p) = hover {
            events.push(egui::Event::PointerMoved(p));
        }
        let mut input = egui::RawInput { events, ..Default::default() };
        let mut count = 0;
        for _ in 0..2 {
            let mut out = ctx.run_ui(std::mem::take(&mut input), |ui| {
                let painter = ui.painter().clone();
                layer.paint(state, view, &painter);
            });
            out.textures_delta.clear();
            count = out.shapes.len();
            if let Some(p) = hover {
                input.events.push(egui::Event::PointerMoved(p));
            }
        }
        count
    }

    #[test]
    fn hovered_or_selected_systems_show_their_effect_name() {
        let handle = new_handle();
        let mut layer = ParticleLayer::new(handle.clone());
        let mut state = state_with_particles(2);
        layer.prepare(&state);
        let view = ViewInfo { cam3d: state.cameras.cam3d, ..view() };
        assert_eq!(painted(&layer, &state, &view, None), 0, "nothing hovered or selected");
        state.doc.select(&[ObjRef::Entity(101)], hp_editor::SelectOp::Replace);
        let selected = painted(&layer, &state, &view, None);
        assert!(selected > 0);
        state.doc.select(&[], hp_editor::SelectOp::Replace);
        let at = view.world_to_screen(DVec3::ZERO).expect("in view");
        assert!(painted(&layer, &state, &view, Some(at)) > 0);
        assert_eq!(painted(&layer, &state, &view, Some(at + egui::vec2(200.0, 200.0))), 0);
        state.doc.select(&[ObjRef::Entity(101)], hp_editor::SelectOp::Replace);
        assert_eq!(painted(&layer, &state, &ViewInfo { kind: hp_app::ViewKind::Top, ..view }, None), 0);
        state.runtime.lab.player.active = true;
        assert_eq!(painted(&layer, &state, &view, None), 0);
    }

    #[test]
    fn sheet_frames_flips_and_orientations() {
        let sheet = Sheet {
            sequences: vec![hp_pcf::sheet::SheetSequence {
                number: 0,
                clamp: false,
                frames: vec![
                    hp_pcf::sheet::SheetFrame { duration: 1.0, uv: [0.0, 0.0, 0.5, 1.0] },
                    hp_pcf::sheet::SheetFrame { duration: 1.0, uv: [0.5, 0.0, 1.0, 1.0] },
                ],
                total: 2.0,
            }],
        };
        let def = hp_pcf::SystemDef {
            name: "x".into(),
            emitters: vec![hp_pcf::Operator { function: "emit_instantaneously".into(), ..Default::default() }],
            renderers: vec![hp_pcf::Operator { function: "render_animated_sprites".into(), ..Default::default() }],
            ..Default::default()
        };
        let mut sim = hp_pcf::Sim::new(Arc::new(Plan::new(&def)), 3);
        sim.step(0.01, [0.0; 3], 10);
        let plan = sim.plan().clone();
        let mut p = sim.particles[0];
        p.age = 1.0;
        assert_eq!(particle_uv(&plan, &p, Some(&sheet)), [0.0, 0.0, 0.5, 1.0]);
        p.age = 6.0;
        assert_eq!(particle_uv(&plan, &p, Some(&sheet)), [0.5, 0.0, 1.0, 1.0]);
        p.flip = true;
        assert_eq!(particle_uv(&plan, &p, Some(&sheet)), [1.0, 0.0, 0.5, 1.0]);
        assert_eq!(particle_uv(&plan, &p, None), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(orientation(0), ParticleOrient::Screen);
        assert_eq!(orientation(1), ParticleOrient::Upright);
        assert_eq!(orientation(2), ParticleOrient::Flat);
    }

    #[test]
    fn materials_give_a_tint_and_a_blend_guess() {
        assert_eq!(material_tint("particle/fire_burning"), [1.0, 0.55, 0.15]);
        assert_eq!(material_tint("effects/Spark"), [1.0, 0.85, 0.4]);
        assert_eq!(material_tint("particle/smoke1/smoke1"), [1.0, 1.0, 1.0]);
        assert!(guess_additive("particle/fire_particle_4/fire_particle_4"));
        assert!(!guess_additive("particle/smoke1/smoke1"));
        assert_eq!(material_key("Materials\\Particle\\Smoke1\\smoke1.VMT"), "particle/smoke1/smoke1");
    }

    #[test]
    fn map_manifests_list_their_files() {
        let text = "particles_manifest\n{\n\t\"file\" \"!particles/My_Map\\\\Fire.pcf\" // fire\n\t\"file\" \"particles/my_map/fire.pcf\"\n\t\"other\" \"x.pcf\"\n\t\"file\" \"readme.txt\"\n\t\"file\"\t\"particles/smoke.pcf\"\n}\n";
        assert_eq!(manifest_files(text), ["particles/my_map/fire.pcf", "particles/smoke.pcf"]);
        assert!(manifest_files("").is_empty());
        assert!(manifest_files("\"file").is_empty());
    }

    fn sheet_vtf() -> Vec<u8> {
        let mut sheet = Vec::new();
        for v in [0u32, 1, 0, 0, 2] {
            sheet.extend_from_slice(&v.to_le_bytes());
        }
        sheet.extend_from_slice(&2.0f32.to_le_bytes());
        for uv in [[0.0f32, 0.0, 0.5, 0.5], [0.5, 0.0, 1.0, 0.5]] {
            sheet.extend_from_slice(&1.0f32.to_le_bytes());
            for c in uv {
                sheet.extend_from_slice(&c.to_le_bytes());
            }
        }
        let mut out = vec![0u8; 88];
        out[..4].copy_from_slice(b"VTF\0");
        out[4..8].copy_from_slice(&7u32.to_le_bytes());
        out[8..12].copy_from_slice(&3u32.to_le_bytes());
        out[68..72].copy_from_slice(&1u32.to_le_bytes());
        out[80..84].copy_from_slice(&[0x10, 0, 0, 0]);
        out[84..88].copy_from_slice(&88u32.to_le_bytes());
        out.extend_from_slice(&(sheet.len() as u32).to_le_bytes());
        out.extend_from_slice(&sheet);
        out
    }

    #[test]
    fn material_looks_come_from_the_vmt_and_the_sheet() {
        let dir = std::env::temp_dir().join(format!("hp_lab_look_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("materials/particle")).expect("dir");
        std::fs::write(
            dir.join("materials/particle/glow.vmt"),
            "SpriteCard { $basetexture particle/glow $additive 1 $overbrightfactor 3 }",
        )
        .expect("vmt");
        std::fs::write(dir.join("materials/particle/glow.vtf"), sheet_vtf()).expect("vtf");
        std::fs::write(
            dir.join("materials/particle/smoke.vmt"),
            "SpriteCard { $basetexture particle/none $addself .5 $overbrightfactor 2 $startfadesize .4 $endfadesize .6 }",
        )
        .expect("vmt");
        std::fs::write(
            dir.join("materials/particle/puff.vmt"),
            "UnlitGeneric { $basetexture particle/none $translucent 1 \"srgb?$alpha\" \".27\" }",
        )
        .expect("vmt");
        std::fs::write(
            dir.join("materials/particle/heat.vmt"),
            "Refract { $normalmap particle/heat $refractamount -.007 $bluramount 1 $nofog 1 }",
        )
        .expect("vmt");
        for (name, shader, flags) in [
            ("second_only", "SpriteCard", "$dualsequence 1 $maxlumframeblend2 1"),
            ("both_from_first", "SpriteCard", "$dualsequence 1 $maxlumframeblend1 1 $maxlumframeblend2 0"),
            ("unlit", "UnlitGeneric", "$dualsequence 1 $maxlumframeblend1 1"),
        ] {
            std::fs::write(dir.join(format!("materials/particle/{name}.vmt")), format!("{shader} {{ {flags} }}"))
                .unwrap();
        }
        let fs = hp_fs::GameFs::empty();
        assert!(fs.add_game_dir(&dir).is_ok());
        let glow = read_material_look(&fs, "Particle\\Glow.vmt");
        assert!(glow.found && glow.additive);
        assert_eq!(glow.overbright, 3.0);
        let sheet = glow.sheet.as_ref().expect("sheet");
        assert_eq!(sheet.frame_count(0), 2);
        assert_eq!(sheet.frame_uv(0, 0.75), [0.5, 0.0, 1.0, 0.5]);
        let puff = read_material_look(&fs, "particle/puff");
        assert!((puff.opacity - 0.27).abs() < 1e-4, "{}", puff.opacity);
        assert_eq!(glow.opacity, 1.0);
        let smoke = read_material_look(&fs, "particle/smoke");
        assert!(smoke.found && !smoke.additive && smoke.sheet.is_none());
        assert_eq!(smoke.blend(), ParticleBlend::AddSelf);
        assert_eq!(smoke.brightness(), 2.0);
        assert_eq!(smoke.fade_size, [0.4, 0.6]);
        assert_eq!(glow.blend(), ParticleBlend::Additive);
        assert_eq!(glow.fade_size, [0.0; 2], "no fade by default");
        let missing = read_material_look(&fs, "particle/fire_missing");
        assert!(!missing.found && missing.additive, "guessed from the name");
        let heat = read_material_look(&fs, "particle/heat");
        assert_eq!(heat.blend(), ParticleBlend::Refract);
        assert_eq!(heat.blend_parameter(), -0.007);
        assert_eq!(heat.refraction_blur, 1.0);
        assert!(heat.no_fog);
        let second = read_material_look(&fs, "particle/second_only");
        assert!(second.dual_sequence);
        assert_eq!(second.max_luminance, [false; 2]);
        let both = read_material_look(&fs, "particle/both_from_first");
        assert_eq!(both.max_luminance, [true; 2]);
        let unlit = read_material_look(&fs, "particle/unlit");
        assert!(!unlit.dual_sequence);
        assert_eq!(unlit.max_luminance, [false; 2]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn material_opacity_stays_in_the_material_instead_of_being_applied_twice() {
        let plan = plan("alpha_test");
        let mut effect = Effect::new("alpha_test", &|_| Some(plan.clone()), 9).expect("effect");
        effect.step(0.1, [0.0; 3], 20);
        let mut sprites = Vec::new();
        let mut materials = Vec::new();
        let looks = HashMap::from([(
            "particle/fire".to_owned(),
            Arc::new(MaterialLook { opacity: 0.25, ..Default::default() }),
        )]);
        append_sprites(std::iter::once((1, &effect)), &looks, &mut sprites, &mut materials);
        let alpha: Vec<_> = effect.particles().map(|(_, p)| p.alpha).collect();
        assert!(!sprites.is_empty());
        assert_eq!(sprites.iter().map(|p| p.color[3]).collect::<Vec<_>>(), alpha);
    }

    #[test]
    fn sprite_adapter_retains_small_nonzero_opacity_until_native_material_blending() {
        let definition = hp_pcf::SystemDef {
            name: "faint".into(),
            material: "particle/faint".into(),
            initial_particles: 1,
            renderers: vec![hp_pcf::Operator { function: "render_animated_sprites".into(), ..Default::default() }],
            ..Default::default()
        };
        let plan = Arc::new(Plan::new(&definition));
        let mut effect = Effect::new("faint", &|_| Some(plan.clone()), 17).unwrap();
        effect.step(0.02, [0.0; 3], 10);
        effect.sims[0].1.particles[0].alpha = 1.0 / 255.0;
        for (spritecard, add_self) in [(false, 0.0), (true, 0.0), (true, 0.5)] {
            let looks = HashMap::from([(
                "particle/faint".into(),
                Arc::new(MaterialLook { spritecard, add_self, ..Default::default() }),
            )]);
            let mut sprites = Vec::new();
            let mut materials = Vec::new();
            append_sprites(std::iter::once((73, &effect)), &looks, &mut sprites, &mut materials);
            assert_eq!(sprites.len(), 1, "the native pixel shader decides whether the combined alpha is visible");
            assert_eq!(sprites[0].color[3], 1.0 / 255.0);
        }
        effect.sims[0].1.particles[0].alpha = 0.0;
        let mut sprites = Vec::new();
        append_sprites(std::iter::once((73, &effect)), &HashMap::new(), &mut sprites, &mut Vec::new());
        assert!(sprites.is_empty());
    }

    #[test]
    fn trail_adapter_preserves_native_width_history_and_small_nonzero_opacity() {
        let definition = hp_pcf::SystemDef {
            name: "trail".into(),
            material: "particle/trail".into(),
            initial_particles: 1,
            renderers: vec![hp_pcf::Operator { function: "render_sprite_trail".into(), ..Default::default() }],
            ..Default::default()
        };
        let mut plan = Plan::new(&definition);
        plan.render.constrain_radius = false;
        plan.render.len_fade_in = 0.0;
        let plan = Arc::new(plan);
        let mut effect = Effect::new("trail", &|_| Some(plan.clone()), 17).unwrap();
        effect.step(0.02, [0.0; 3], 10);
        let particle = &mut effect.sims[0].1.particles[0];
        particle.pos = [2.0, 3.0, 4.0];
        particle.vel = [20.0, 0.0, 0.0];
        particle.age = 1.0;
        particle.trail = 3.0;
        particle.radius = 0.02;
        particle.alpha = 1.0 / 255.0;
        particle.rotation = 0.7;
        particle.yaw = 0.4;
        for spritecard in [false, true] {
            let looks =
                HashMap::from([("particle/trail".into(), Arc::new(MaterialLook { spritecard, ..Default::default() }))]);
            let mut sprites = Vec::new();
            let mut materials = Vec::new();
            append_sprites(std::iter::once((73, &effect)), &looks, &mut sprites, &mut materials);
            assert_eq!(sprites.len(), 1, "nonzero native byte opacity must survive the adapter");
            let sprite = &sprites[0];
            let particle = &effect.sims[0].1.particles[0];
            let path = if spritecard { hp_pcf::trail::Path::Spline } else { hp_pcf::trail::Path::Cpu };
            let expected = plan.render.trail_segment(particle, 0.02, path).unwrap();
            assert_eq!(sprite.pos, particle.pos);
            assert_eq!(sprite.axis, expected.delta);
            assert_eq!(sprite.size, particle.radius, "trail radius is its full width");
            assert_eq!(sprite.color[3], particle.opacity());
            assert_eq!(sprite.roll, 0.0);
            assert_eq!(sprite.yaw, 0.0);
            assert_eq!(sprite.orient, if spritecard { ParticleOrient::TrailSpline } else { ParticleOrient::TrailCpu });
            assert_eq!(sprite.entity, 73);
            assert_eq!(materials, ["particle/trail"]);
        }
    }

    #[test]
    fn particle_files_load_from_the_game_content() {
        let dir = std::env::temp_dir().join(format!("hp_lab_pcf_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("particles")).expect("dir");
        std::fs::write(dir.join("particles").join("bad.pcf"), b"not a pcf").expect("write");
        let fs = hp_fs::GameFs::empty();
        assert!(fs.add_game_dir(&dir).is_ok());
        assert!(load_file(&fs, "particles/bad.pcf").is_err());
        assert!(load_file(&fs, "particles/missing.pcf").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exported_sprite_opacity_matches_native_alpha2_samples_before_pixel_clipping() {
        let definitions = hp_pcf::parse_file(include_bytes!("../../hp-pcf/tests/data/alpha2_cases.pcf")).unwrap();
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../hp-pcf/tests/data/alpha2_native_samples.json")).unwrap();
        for case in cases {
            let name = format!("hp_spritecard_{}", case["name"].as_str().unwrap());
            let definition = definitions.iter().find(|def| def.name == name).unwrap();
            let plan = Arc::new(Plan::new(definition));
            let mut effect = Effect::new(&name, &|_| Some(plan.clone()), 17).unwrap();
            effect.step(0.0, [0.0; 3], 1);
            let mut sprites = Vec::new();
            let mut materials = Vec::new();
            let looks = HashMap::from([(plan.material.clone(), Arc::new(MaterialLook::default()))]);
            append_sprites(std::iter::once((1, &effect)), &looks, &mut sprites, &mut materials);
            // Retain every positive byte opacity. The native material's pixel
            // cutoff is covered by the renderer's alpha2 reference test.
            let submitted = case["opacity"].as_f64().unwrap() > 0.0;
            assert_eq!(sprites.len(), usize::from(submitted), "{name}");
            if submitted {
                assert_eq!(sprites[0].color[3], case["opacity"].as_f64().unwrap() as f32, "{name}");
            }
        }
    }

    #[test]
    fn stock_gmod_effects_play() {
        let root = std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod");
        if !root.join("garrysmod").is_dir() {
            return;
        }
        let opts = hp_fs::MountOptions {
            mount_addons: false,
            mount_workshop: false,
            mount_cfg: false,
            ..hp_fs::MountOptions::default()
        };
        let Ok(fs) = hp_fs::GameFs::for_gmod(root, &opts) else { return };
        let mut shared = ParticleShared::default();
        for file in ["particles/fire_01.pcf", "particles/blood_impact.pcf", "particles/water_impact.pcf"] {
            let loaded = load_file(&fs, file).expect(file);
            for p in loaded.plans {
                shared.library.insert(p.name.to_ascii_lowercase(), p);
            }
            for (k, l) in loaded.looks {
                shared.looks.insert(k, l);
            }
        }
        let fire = shared.looks.get("particle/fire_particle_4/fire_particle_4").expect("fire look");
        assert!(fire.found);
        let sheets = shared.looks.values().filter(|l| l.sheet.is_some()).count();
        let additive = shared.looks.values().filter(|l| l.additive).count();
        eprintln!("{} materials: {sheets} with a sprite sheet, {additive} additive", shared.looks.len());
        assert!(sheets > 0, "the sprite sheets of the textures are read");
        for name in [
            "fire_small_01",
            "fire_medium_01",
            "smoke_burning_engine_01",
            "env_fire_large",
            "blood_impact_red_01",
            "water_splash_01",
        ] {
            let s = Source {
                id: 1,
                name: name.into(),
                label: name.into(),
                origin: DVec3::ZERO,
                points: Points::at([0.0; 3]),
                model_objects: Default::default(),
            };
            let playing = ParticleLayer::start(&mut shared, &s).unwrap_or_else(|| panic!("{name}"));
            let mut layer = ParticleLayer {
                lighting: None,
                handle: new_handle(),
                sources: Vec::new(),
                sources_key: None,
                effects: HashMap::from([(1, playing)]),
            };
            let mut peak_sprites = 0;
            // Short bursts can finish fading before one second; verify their rendered lifetime.
            for _ in 0..20 {
                layer.effects.get_mut(&1).unwrap().effect.step_at(0.05, &s.points, MAX_PER_EFFECT);
                layer.fill_sprites(&mut shared, [-300.0, 0.0, 0.0]);
                peak_sprites = peak_sprites.max(shared.sprites.len());
            }
            let unsupported: Vec<&String> =
                layer.effects[&1].effect.sims.iter().flat_map(|(_, s)| &s.plan().unsupported).collect();
            eprintln!("{name}: {peak_sprites} peak sprites, unsupported {unsupported:?}");
            assert!(peak_sprites > 0, "{name} draws");
        }
    }
}

#[cfg(test)]
#[path = "particles_native_tests.rs"]
mod native_tests;
