use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use glam::DVec3;
use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_editor::{Id, ObjRef};
use hp_pcf::{Effect, Plan, Points, RenderKind, Sheet};
use hp_render::{Overlay, OverlayTri, ParticleBlend, ParticleOrient};
use hp_ui_viewports::layers::OverlayLayer;

pub const MAX_EFFECTS: usize = 24;
pub const MAX_PARTICLES: usize = 3000;
pub const MAX_PER_EFFECT: usize = 400;
pub const MAX_DISTANCE: f64 = 4096.0;
const STARTS_PER_FRAME: usize = 4;
const WARM_UP: f32 = 1.5;
const HOVER_PX: f32 = 14.0;
const BSP_ID_BASE: Id = 0xF000_0000;

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialLook {
    pub found: bool,
    pub additive: bool,
    pub overbright: f32,
    pub add_self: f32,
    pub fade_size: [f32; 2],
    pub opacity: f32,
    pub blend_frames: bool,
    pub sheet: Option<Sheet>,
    pub refraction: Option<f32>,
    pub refraction_blur: f32,
    pub no_fog: bool,
}

impl Default for MaterialLook {
    fn default() -> Self {
        Self {
            found: false,
            additive: false,
            overbright: 1.0,
            add_self: 0.0,
            fade_size: [0.0; 2],
            opacity: 1.0,
            blend_frames: false,
            sheet: None,
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
    pub physics: Arc<crate::particle_world::Physics>,
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
    pub materials: Vec<String>,
    pub fx_sprites: Vec<ParticleSprite>,
    pub fx_materials: Vec<String>,
    pub playing: Vec<(Id, String)>,
    pub particles: usize,
}

impl ParticleShared {
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
    pub pos: [f32; 3],
    pub size: f32,
    pub axis: [f32; 3],
    pub roll: f32,
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
    let blend_frames = m.shader.eq_ignore_ascii_case("spritecard") && m.get_i32("$blendframes").unwrap_or(1) != 0;
    MaterialLook {
        found: true,
        additive: m.flag("$additive") || sprite_mode,
        overbright,
        add_self,
        fade_size,
        opacity,
        blend_frames,
        sheet,
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
    let mut uv = match sheet {
        Some(sheet) => {
            let total = sheet.sequence(p.sequence).map_or(0.0, |s| s.total);
            sheet.frame_uv(p.sequence, plan.cycle(p, total))
        }
        None => hp_pcf::sheet::FULL_UV,
    };
    if p.flip {
        uv.swap(0, 2);
    }
    uv
}

pub fn particle_uv_blend(plan: &Plan, p: &hp_pcf::Particle, sheet: &Sheet) -> ([f32; 4], [f32; 4], f32) {
    let total = sheet.sequence(p.sequence).map_or(0.0, |s| s.total);
    let (mut a, mut b, f) = sheet.frame_blend(p.sequence, plan.cycle(p, total));
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
        for (plan, particles) in effect.systems() {
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
            let render = plan.render;
            let base = |p: &hp_pcf::Particle| ParticleSprite {
                pos: p.pos,
                size: p.radius * 2.0,
                axis: [0.0; 3],
                roll: p.rotation,
                uv: particle_uv(plan, p, look.sheet.as_ref()),
                color: [p.color[0], p.color[1], p.color[2], p.alpha.clamp(0.0, 1.0)],
                bright,
                add_self,
                orient: orientation(render.orient),
                blend,
                screen_fade: look.fade_size,
                refraction_blur: look.refraction_blur,
                no_fog: look.no_fog,
                material,
                entity: id,
            };
            let visible =
                |p: &hp_pcf::Particle| p.alpha > 0.004 && p.radius > 0.0 && p.pos.iter().all(|v| v.is_finite());
            match render.kind {
                RenderKind::Sprites => {
                    for p in particles.iter().filter(|p| visible(p)) {
                        let mut s = base(p);
                        if render.orient == 3 {
                            s.axis = [0.0, 0.0, 1.0];
                        }
                        push_blended(sprites, s, plan, p, look);
                    }
                }
                RenderKind::Trail => {
                    for p in particles.iter().filter(|p| visible(p)) {
                        let speed = (p.vel[0] * p.vel[0] + p.vel[1] * p.vel[1] + p.vel[2] * p.vel[2]).sqrt();
                        let mut len = (speed * p.trail).clamp(render.min_len, render.max_len.max(render.min_len));
                        if render.len_fade_in > 0.0 {
                            len *= (p.age / render.len_fade_in).clamp(0.0, 1.0);
                        }
                        if speed < 1e-3 || len < 0.05 {
                            continue;
                        }
                        let d = p.vel.map(|c| c / speed);
                        let mut s = base(p);
                        s.pos = [p.pos[0] - d[0] * len * 0.5, p.pos[1] - d[1] * len * 0.5, p.pos[2] - d[2] * len * 0.5];
                        s.axis = d.map(|c| c * len);
                        s.orient = ParticleOrient::Stretched;
                        s.roll = 0.0;
                        push_blended(sprites, s, plan, p, look);
                    }
                }
                RenderKind::Rope => {
                    for w in particles.windows(2) {
                        let (a, b) = (&w[0], &w[1]);
                        if !visible(a) || !visible(b) {
                            continue;
                        }
                        let axis = [b.pos[0] - a.pos[0], b.pos[1] - a.pos[1], b.pos[2] - a.pos[2]];
                        let mut s = base(a);
                        s.pos = [(a.pos[0] + b.pos[0]) * 0.5, (a.pos[1] + b.pos[1]) * 0.5, (a.pos[2] + b.pos[2]) * 0.5];
                        s.size = a.radius + b.radius;
                        s.axis = axis;
                        s.orient = ParticleOrient::Stretched;
                        s.roll = 0.0;
                        s.uv = hp_pcf::sheet::FULL_UV;
                        s.color[3] = (a.alpha + b.alpha) * 0.5;
                        sprites.push(s);
                    }
                }
                RenderKind::None => {}
            }
        }
    }
}

fn push_blended(
    sprites: &mut Vec<ParticleSprite>,
    mut s: ParticleSprite,
    plan: &Plan,
    p: &hp_pcf::Particle,
    look: &MaterialLook,
) {
    let Some(sheet) = look.sheet.as_ref().filter(|_| look.blend_frames) else {
        sprites.push(s);
        return;
    };
    let (a, b, f) = particle_uv_blend(plan, p, sheet);
    if f <= 0.01 || a == b {
        sprites.push(ParticleSprite { uv: a, ..s });
        return;
    }
    let alpha = s.color[3];
    sprites.push(ParticleSprite { uv: a, color: [s.color[0], s.color[1], s.color[2], alpha * (1.0 - f)], ..s });
    s.uv = b;
    s.color[3] = alpha * f;
    sprites.push(s);
}

pub fn sort_far_first(sprites: &mut [ParticleSprite], eye: [f32; 3]) {
    let d2 = |s: &ParticleSprite| {
        let d = [s.pos[0] - eye[0], s.pos[1] - eye[1], s.pos[2] - eye[2]];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
    };
    sprites.sort_by(|a, b| d2(b).total_cmp(&d2(a)));
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
                pos: p.pos,
                size: p.size,
                axis: p.axis,
                roll: p.roll,
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
}

fn parse3(s: Option<&str>) -> Option<[f32; 3]> {
    let v: Vec<f32> = s?.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    (v.len() == 3 && v.iter().all(|x| x.is_finite())).then(|| [v[0], v[1], v[2]])
}

fn f3(v: DVec3) -> [f32; 3] {
    [v.x as f32, v.y as f32, v.z as f32]
}

fn sources_of<'a>(entities: impl Iterator<Item = (Id, &'a hp_vmf::Entity)> + Clone) -> Vec<Source> {
    let is_system = |e: &hp_vmf::Entity| e.classname.eq_ignore_ascii_case("info_particle_system");
    let wants_points = entities.clone().any(|(_, e)| {
        is_system(e)
            && e.props.iter().any(|(k, v)| k.to_ascii_lowercase().starts_with("cpoint") && !v.trim().is_empty())
    });
    let mut named: HashMap<String, (DVec3, [f32; 3])> = HashMap::new();
    if wants_points {
        for (_, e) in entities.clone() {
            if let (Some(n), Some(o)) = (e.get("targetname"), e.origin()) {
                let n = n.trim().to_ascii_lowercase();
                if !n.is_empty() && o.is_finite() {
                    named.entry(n).or_insert((o, parse3(e.get("angles")).unwrap_or([0.0; 3])));
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
        if !named.is_empty() {
            for k in 1..hp_pcf::sim::MAX_CONTROL_POINTS {
                let Some(target) = e.get(&format!("cpoint{k}")).map(|t| t.trim().to_ascii_lowercase()) else {
                    continue;
                };
                let Some((p, angles)) = named.get(&target) else { continue };
                if points.pos.len() <= k {
                    points.pos.resize(k + 1, f3(origin));
                }
                points.pos[k] = f3(*p);
                if points.orientations.len() <= k {
                    points.orientations.resize(k + 1, Points::at([0.0; 3]).axes);
                }
                points.orientations[k] = Points::with_angles(f3(*p), *angles).axes;
            }
        }
        out.push(Source { id, name: name.to_ascii_lowercase(), label: name.to_owned(), origin, points });
    }
    out
}

impl ParticleLayer {
    pub fn new(handle: ParticleHandle) -> Self {
        Self { handle, sources: Vec::new(), sources_key: None, effects: HashMap::new() }
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
                let list: Vec<(Id, &hp_vmf::Entity)> = w
                    .particle_entities
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        let id = e.get("hammerid").and_then(|v| v.trim().parse::<Id>().ok());
                        (id.unwrap_or(BSP_ID_BASE + i as Id), e)
                    })
                    .collect();
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
        let ParticleShared { sprites, materials, looks, .. } = shared;
        sprites.clear();
        materials.clear();
        append_sprites(self.effects.iter().map(|(id, p)| (*id, &p.effect)), looks, sprites, materials);
        sort_far_first(sprites, eye);
    }

    fn stop_all(&mut self) {
        self.effects.clear();
        if let Ok(mut s) = self.handle.lock() {
            s.active = false;
            s.playing.clear();
            s.particles = 0;
            s.sprites.clear();
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
        let physics = Arc::clone(&shared.physics);
        let world = crate::particle_world::ParticleEnvironment { state, physics: &physics };
        let keep: HashSet<Id> = near.iter().map(|(_, s)| s.id).collect();
        self.effects.retain(|id, _| keep.contains(id));
        let dt = state.frame.dt.clamp(0.0, 0.1);
        let mut budget = MAX_PARTICLES;
        let mut starts = 0;
        for (_, s) in &near {
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
                if p.alpha <= 0.01 || p.radius <= 0.0 {
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
                    (p.alpha * SPRITE_COVERAGE).clamp(0.0, 1.0),
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
            assert!(s.sprites.windows(2).all(|w| d(&w[0]) >= d(&w[1])), "far to near");
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
        sys.set("origin", "0 0 0");
        sys.set("angles", "0 90 0");
        sys.set("cpoint1", "Target");
        sys.set("cpoint3", "nobody");
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
        assert_eq!(s.points.get(3), [0.0, 0.0, 0.0]);
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
            };
            let mut playing = ParticleLayer::start(&mut shared, &s).unwrap_or_else(|| panic!("{name}"));
            for _ in 0..20 {
                playing.effect.step_at(0.05, &s.points, MAX_PER_EFFECT);
            }
            let layer = ParticleLayer {
                handle: new_handle(),
                sources: Vec::new(),
                sources_key: None,
                effects: HashMap::from([(1, playing)]),
            };
            layer.fill_sprites(&mut shared, [-300.0, 0.0, 0.0]);
            let unsupported: Vec<&String> =
                layer.effects[&1].effect.sims.iter().flat_map(|(_, s)| &s.plan().unsupported).collect();
            eprintln!("{name}: {} sprites, unsupported {unsupported:?}", shared.sprites.len());
            assert!(!shared.sprites.is_empty(), "{name} draws");
        }
    }
}
