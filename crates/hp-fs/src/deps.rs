mod pcf;
mod sounds;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use parking_lot::Mutex;
use rayon::prelude::*;

pub use pcf::{ParticleFile, parse_pcf};
pub use sounds::{
    ScriptEntry, SoundIndex, Soundscape, SoundscapeIndex, is_sound_file, manifest_files, sound_path, strip_sound_chars,
};

use crate::custom::CustomHit;
use crate::error::FsError;
use crate::export::{Outcome, Payload, materialize, payload_of, write_if_changed};
use crate::fs::GameFs;
use crate::path::{normalize_path, safe_join, strip_extension};
use crate::source::{Source, SourceData, SourceOrigin};
use crate::texture::{TextureHit, find_texture};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RootKind {
    Material,
    Texture,
    Model,
    Sprite,
    Sound,
    Particle,
    Soundscape,
    Skybox,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepRoot {
    pub kind: RootKind,
    pub name: String,
    pub why: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DepKind {
    Material,
    Texture,
    Model,
    ModelPart,
    Sound,
    SoundScript,
    Soundscape,
    Particle,
    ParticleManifest,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DepReason {
    Root(String),
    UsedBy(String),
    Generated,
}

pub const MAX_REASONS: usize = 8;

#[derive(Clone, Debug)]
pub struct DepNode {
    pub vpath: String,
    pub kind: DepKind,
    pub source: Option<u32>,
    pub origin: Option<SourceOrigin>,
    pub source_label: String,
    pub workshop_id: Option<u64>,
    pub in_base: bool,
    pub size: u64,
    pub reasons: Vec<DepReason>,
    pub reason_count: usize,
    pub generated: Option<Arc<Vec<u8>>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackPolicy {
    pub workshop: bool,
    pub mounted: bool,
}

impl Default for PackPolicy {
    fn default() -> Self {
        PackPolicy { workshop: true, mounted: false }
    }
}

impl DepNode {
    pub fn ships(&self, policy: PackPolicy) -> bool {
        if self.generated.is_some() {
            return true;
        }
        match self.origin {
            Some(SourceOrigin::GameLoose) => !self.in_base,
            Some(SourceOrigin::Workshop) => policy.workshop,
            Some(SourceOrigin::Mounted) => policy.mounted,
            Some(o) => o.needs_packing(),
            None => false,
        }
    }

    fn add_reason(&mut self, reason: &DepReason) {
        if self.reasons.contains(reason) {
            return;
        }
        self.reason_count += 1;
        if self.reasons.len() < MAX_REASONS {
            self.reasons.push(reason.clone());
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingAsset {
    pub kind: DepKind,
    pub name: String,
    pub reasons: Vec<DepReason>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum DepWarning {
    MountedContent { vpath: String, source: String },
    ScriptNotLoaded { name: String, file: String },
    SoundscapeNotLoaded { name: String, file: String },
    Unreadable { vpath: String, error: String },
}

#[derive(Clone, Debug, Default)]
pub struct DependencyGraph {
    pub nodes: Vec<DepNode>,
    pub missing: Vec<MissingAsset>,
    pub warnings: Vec<DepWarning>,
    pub generation: u64,
    pub cancelled: bool,
    pub links: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OriginSummary {
    pub origin: Option<SourceOrigin>,
    pub files: usize,
    pub bytes: u64,
}

impl DependencyGraph {
    pub fn find(&self, vpath: &str) -> Option<&DepNode> {
        let key = normalize_path(vpath);
        self.nodes.binary_search_by(|n| n.vpath.as_str().cmp(&key)).ok().map(|i| &self.nodes[i])
    }

    pub fn shipped(&self, policy: PackPolicy) -> impl Iterator<Item = &DepNode> {
        self.nodes.iter().filter(move |n| n.ships(policy))
    }

    pub fn by_origin(&self) -> Vec<OriginSummary> {
        let mut map: HashMap<Option<SourceOrigin>, (usize, u64)> = HashMap::new();
        for n in &self.nodes {
            let e = map.entry(n.origin).or_default();
            e.0 += 1;
            e.1 += n.size;
        }
        let mut out: Vec<OriginSummary> =
            map.into_iter().map(|(origin, (files, bytes))| OriginSummary { origin, files, bytes }).collect();
        out.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.origin.cmp(&b.origin)));
        out
    }

    pub fn children_of<'a>(&'a self, parent: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        let start = self.links.partition_point(|(p, _)| p.as_str() < parent);
        self.links[start..].iter().take_while(move |(p, _)| p == parent).map(|(_, c)| c.as_str())
    }

    pub fn workshop_items(&self) -> Vec<(u64, String)> {
        let mut out: Vec<(u64, String)> =
            self.nodes.iter().filter_map(|n| n.workshop_id.map(|id| (id, n.source_label.clone()))).collect();
        out.sort_unstable();
        out.dedup_by_key(|x| x.0);
        out
    }
}

#[derive(Clone, Debug)]
enum Parsed {
    Children(Vec<(Req, String)>),
    Particles(Arc<ParticleFile>),
    Unreadable(String),
}

type Stamp = Option<(u64, SystemTime)>;

#[derive(Default)]
struct Indexes {
    key: Option<(u64, String)>,
    sounds: Option<Arc<SoundIndex>>,
    scapes: Option<Arc<SoundscapeIndex>>,
    particles: Option<Arc<ParticleIndex>>,
}

#[derive(Default)]
pub struct DepsCache {
    parsed: Mutex<HashMap<(u32, String), (Stamp, Parsed)>>,
    indexes: Mutex<Indexes>,
}

impl std::fmt::Debug for DepsCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DepsCache").field("parsed", &self.parsed.lock().len()).finish()
    }
}

impl DepsCache {
    pub fn new() -> DepsCache {
        DepsCache::default()
    }

    pub fn len(&self) -> usize {
        self.parsed.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ResolveOptions<'a> {
    pub map_name: &'a str,
    pub cancel: Option<&'a AtomicBool>,
    pub editor_textures: bool,
}

#[derive(Debug, Default)]
struct ParticleIndex {
    systems: HashMap<String, String>,
    files: HashMap<String, Arc<ParticleFile>>,
    in_base_manifest: HashSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Req {
    Material,
    OptionalMaterial,
    Texture,
    OptionalTexture,
    EditorTexture,
    Model,
    Sound,
    ModelSound,
    Soundscape,
    Particle,
    Skybox,
    File(DepKind),
}

#[derive(Clone)]
struct Request {
    req: Req,
    name: String,
    reason: DepReason,
}

struct Hit<'a> {
    vpath: String,
    kind: DepKind,
    source: &'a Source,
    size: u64,
}

#[derive(Default)]
struct Expansion<'a> {
    hits: Vec<Hit<'a>>,
    children: Vec<(Req, String)>,
    missing: Vec<(DepKind, String)>,
    warnings: Vec<DepWarning>,
    primary: Option<String>,
}

const TEXTURE_KEYS: &[&str] = &[
    "$basetexture",
    "$basetexture2",
    "$basetexture3",
    "$basetexture4",
    "$bumpmap",
    "$bumpmap2",
    "$normalmap",
    "$normalmap2",
    "$detail",
    "$detail2",
    "$envmapmask",
    "$selfillummask",
    "$phongexponenttexture",
    "$phongwarptexture",
    "$lightwarptexture",
    "$blendmodulatetexture",
    "$dudvmap",
    "$texture2",
    "$iris",
    "$corneatexture",
    "$ambientoccltexture",
    "$tintmasktexture",
    "$flowmap",
    "$flow_noise_texture",
    "$refracttexture",
    "$reflecttexture",
    "$maskstexture",
    "$specmasktexture",
    "$emissiveblendtexture",
    "$emissiveblendbasetexture",
    "$emissiveblendflowtexture",
    "$hdrbasetexture",
    "$hdrcompressedtexture",
    "$basetexturenormal",
];

/// Known VMT texture variables, excluding similarly named scalar and transform variables.
pub fn is_material_texture_key(key: &str) -> bool {
    let key = key.rsplit('?').next().unwrap_or(key);
    key.eq_ignore_ascii_case("$envmap") || TEXTURE_KEYS.iter().any(|known| key.eq_ignore_ascii_case(known))
}

const MATERIAL_KEYS: &[&str] =
    &["include", "$bottommaterial", "$underwateroverlay", "$crackmaterial", "$reflectmaterial"];

const OPTIONAL_MATERIAL_KEYS: &[&str] = &["$fallbackmaterial"];

const NON_FILE_KEYS: &[&str] =
    &["$surfaceprop", "$surfaceprop2", "%keywords", "$model", "%tooltexture", "%compilekeywords"];

const MODEL_PARTS: &[&str] = &[".vvd", ".dx90.vtx", ".dx80.vtx", ".vtx", ".phy", ".ani"];

const SKY_FACES: [&str; 6] = ["up", "dn", "lf", "rt", "ft", "bk"];

fn not_a_file(value: &str) -> bool {
    let v = value.trim();
    v.is_empty()
        || v.starts_with(['[', '{', '$', '%', '-', '.'])
        || v.parse::<f64>().is_ok()
        || v.eq_ignore_ascii_case("env_cubemap")
        || v.contains(' ')
        || v.to_ascii_lowercase().starts_with("_rt_")
}

fn generated_name(name: &str) -> bool {
    name.starts_with("maps/") || name.starts_with("_rt_") || name.starts_with("__")
}

fn workshop_id(source: &Source) -> Option<u64> {
    if source.origin != SourceOrigin::Workshop {
        return None;
    }
    source.path.parent()?.file_name()?.to_str()?.parse().ok()
}

fn file_size(source: &Source, vpath: &str) -> u64 {
    let disk = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    match &source.data {
        SourceData::Dir(d) => d.disk_path(vpath).map_or(0, |p| disk(&p)),
        SourceData::Vpk(v) => v.entry(vpath).map_or(0, |e| e.size()),
        SourceData::Gma(g) => g.entry(vpath).map_or(0, |e| e.size()),
        SourceData::Custom(c) => match c.lookup(vpath) {
            Some(CustomHit::File(p)) => disk(&p),
            Some(CustomHit::Synth(image)) => c.synth_vmt(image).len() as u64,
            None => 0,
        },
    }
}

fn stamp(source: &Source, vpath: &str) -> Stamp {
    let path = source.disk_path(vpath)?;
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

struct Ctx<'a> {
    snapshot: &'a [Arc<Source>],
    base: Vec<&'a Source>,
    map: String,
    cache: &'a DepsCache,
    sounds: Option<Arc<SoundIndex>>,
    scapes: Option<Arc<SoundscapeIndex>>,
    particles: Option<Arc<ParticleIndex>>,
    editor_textures: bool,
}

impl<'a> Ctx<'a> {
    fn find(&self, vpath: &str) -> Option<&'a Source> {
        self.snapshot.iter().find(|s| s.is_enabled() && s.contains(vpath)).map(|s| &**s)
    }

    fn hit(&self, vpath: String, kind: DepKind, source: &'a Source) -> Hit<'a> {
        let size = file_size(source, &vpath);
        Hit { vpath, kind, source, size }
    }

    fn parsed(&self, source: &Source, vpath: &str, parse: impl FnOnce(&[u8]) -> Parsed) -> Parsed {
        let key = (source.id, vpath.to_owned());
        let st = stamp(source, vpath);
        if let Some((s, p)) = self.cache.parsed.lock().get(&key)
            && *s == st
        {
            return p.clone();
        }
        let value = match source.read_cow(vpath) {
            Some(bytes) => parse(&bytes),
            None => Parsed::Unreadable("cannot read the file".to_owned()),
        };
        self.cache.parsed.lock().insert(key, (st, value.clone()));
        value
    }

    fn expand(&self, r: &Request) -> Expansion<'a> {
        match r.req {
            Req::Material | Req::OptionalMaterial => self.material(&r.name, r.req == Req::Material),
            Req::Texture | Req::OptionalTexture | Req::EditorTexture => self.texture(&r.name, r.req == Req::Texture),
            Req::Model => self.model(&r.name),
            Req::Sound => self.sound(&r.name, true),
            Req::ModelSound => self.sound(&r.name, false),
            Req::Soundscape => self.soundscape(&r.name),
            Req::Particle => self.particle(&r.name),
            Req::Skybox => {
                let mut e = Expansion::default();
                for face in SKY_FACES {
                    e.children.push((Req::Material, format!("skybox/{}{face}", r.name)));
                    e.children.push((Req::OptionalMaterial, format!("skybox/{}_hdr{face}", r.name)));
                }
                e
            }
            Req::File(kind) => self.file(&r.name, kind),
        }
    }

    fn material(&self, name: &str, required: bool) -> Expansion<'a> {
        let mut e = Expansion::default();
        if name.is_empty() || generated_name(name) {
            return e;
        }
        let vpath = format!("materials/{name}.vmt");
        let Some(source) = self.find(&vpath) else {
            if required {
                e.missing.push((DepKind::Material, vpath));
            }
            return e;
        };
        let parsed = self.parsed(source, &vpath, |bytes| match hp_vmt::parse_bytes(bytes) {
            Ok(m) => Parsed::Children(material_children(&m)),
            Err(err) => Parsed::Unreadable(err.to_string()),
        });
        match parsed {
            Parsed::Children(c) => {
                e.children = c.into_iter().filter(|(r, _)| *r != Req::EditorTexture || self.editor_textures).collect();
            }
            Parsed::Unreadable(error) => e.warnings.push(DepWarning::Unreadable { vpath: vpath.clone(), error }),
            Parsed::Particles(_) => {}
        }
        e.hits.push(self.hit(vpath.clone(), DepKind::Material, source));
        e.primary = Some(vpath);
        e
    }

    fn texture(&self, name: &str, required: bool) -> Expansion<'a> {
        let mut e = Expansion::default();
        if name.is_empty() || generated_name(name) {
            return e;
        }
        match find_texture(self.snapshot, name) {
            Some(TextureHit::Vtf { source, path }) => e.hits.push(self.hit(path, DepKind::Texture, source)),
            Some(TextureHit::Image { source, index, image }) => {
                let size = std::fs::metadata(index.root().join(&*image.rel)).map(|m| m.len()).unwrap_or(0);
                e.hits.push(Hit { vpath: format!("materials/{name}.vtf"), kind: DepKind::Texture, source, size });
            }
            None if required => e.missing.push((DepKind::Texture, format!("materials/{name}.vtf"))),
            None => {}
        }
        e
    }

    fn model(&self, vpath: &str) -> Expansion<'a> {
        let mut e = Expansion::default();
        let Some(source) = self.find(vpath) else {
            e.missing.push((DepKind::Model, vpath.to_owned()));
            return e;
        };
        e.hits.push(self.hit(vpath.to_owned(), DepKind::Model, source));
        e.primary = Some(vpath.to_owned());
        let stem = strip_extension(vpath);
        let mut has_vtx = false;
        for ext in MODEL_PARTS {
            let part = format!("{stem}{ext}");
            if *ext == ".vtx" && has_vtx {
                continue;
            }
            if let Some(s) = self.find(&part) {
                has_vtx |= ext.ends_with(".vtx");
                e.hits.push(self.hit(part, DepKind::ModelPart, s));
            } else if *ext == ".vvd" {
                e.missing.push((DepKind::ModelPart, part));
            }
        }
        if !has_vtx {
            e.missing.push((DepKind::ModelPart, format!("{stem}.dx90.vtx")));
        }
        let phy = format!("{stem}.phy");
        if let Some(s) = self.find(&phy) {
            let gibs = self.parsed(s, &phy, |bytes| {
                Parsed::Children(hp_mdl::gib_models(bytes).into_iter().map(|g| (Req::Model, g)).collect())
            });
            if let Parsed::Children(gibs) = gibs {
                e.children.extend(gibs.into_iter().filter(|(_, g)| g != vpath));
            }
        }
        let parsed = self.parsed(source, vpath, |bytes| match hp_mdl::read_dependencies(bytes) {
            Ok(deps) => {
                let mut children: Vec<(Req, String)> = Vec::new();
                for candidates in deps.material_candidates() {
                    children.push((Req::Material, candidates.join("\n")));
                }
                children.extend(deps.include_models.into_iter().map(|m| (Req::Model, m)));
                children.extend(deps.sounds.into_iter().map(|s| (Req::ModelSound, s)));
                Parsed::Children(children)
            }
            Err(err) => Parsed::Unreadable(err.to_string()),
        });
        match parsed {
            Parsed::Children(children) => {
                for (req, name) in children {
                    if req == Req::Material && name.contains('\n') {
                        let candidates: Vec<&str> = name.split('\n').collect();
                        let found = candidates.iter().find(|c| self.find(&format!("materials/{c}.vmt")).is_some());
                        match found {
                            Some(c) => e.children.push((Req::Material, (*c).to_owned())),
                            None => e.missing.push((DepKind::Material, format!("materials/{}.vmt", candidates[0]))),
                        }
                    } else {
                        e.children.push((req, name));
                    }
                }
            }
            Parsed::Unreadable(error) => e.warnings.push(DepWarning::Unreadable { vpath: vpath.to_owned(), error }),
            Parsed::Particles(_) => {}
        }
        e
    }

    fn sound(&self, name: &str, required: bool) -> Expansion<'a> {
        let mut e = Expansion::default();
        if is_sound_file(name) {
            if let Some(path) = sound_path(name) {
                if required {
                    e.children.push((Req::File(DepKind::Sound), path));
                } else if let Some(source) = self.find(&path) {
                    e.hits.push(self.hit(path, DepKind::Sound, source));
                }
            }
            return e;
        }
        let Some(key) = strip_sound_chars(name) else { return e };
        let entry = self.sounds.as_ref().and_then(|s| s.entries.get(&key));
        let Some(entry) = entry else {
            if required {
                e.missing.push((DepKind::Sound, name.trim().to_owned()));
            }
            return e;
        };
        if let Some(source) = self.find(&entry.file) {
            if source.origin.needs_packing() && entry.file != format!("maps/{}_level_sounds.txt", self.map) {
                e.warnings.push(DepWarning::ScriptNotLoaded { name: name.trim().to_owned(), file: entry.file.clone() });
            }
            e.hits.push(self.hit(entry.file.clone(), DepKind::SoundScript, source));
            e.primary = Some(entry.file.clone());
        }
        for wave in &entry.waves {
            if let Some(path) = sound_path(wave) {
                e.children.push((Req::File(DepKind::Sound), path));
            }
        }
        e
    }

    fn soundscape(&self, name: &str) -> Expansion<'a> {
        let mut e = Expansion::default();
        let key = name.trim().to_ascii_lowercase();
        let Some(scape) = self.scapes.as_ref().and_then(|s| s.entries.get(&key)) else {
            e.missing.push((DepKind::Soundscape, name.trim().to_owned()));
            return e;
        };
        if let Some(source) = self.find(&scape.file) {
            if source.origin.needs_packing() && scape.file != format!("scripts/soundscapes_{}.txt", self.map) {
                e.warnings
                    .push(DepWarning::SoundscapeNotLoaded { name: name.trim().to_owned(), file: scape.file.clone() });
            }
            e.hits.push(self.hit(scape.file.clone(), DepKind::Soundscape, source));
            e.primary = Some(scape.file.clone());
        }
        for wave in &scape.waves {
            if let Some(path) = sound_path(wave) {
                e.children.push((Req::File(DepKind::Sound), path));
            }
        }
        e.children.extend(scape.scripts.iter().map(|s| (Req::Sound, s.clone())));
        e.children.extend(scape.nested.iter().map(|s| (Req::Soundscape, s.clone())));
        e
    }

    fn particle(&self, name: &str) -> Expansion<'a> {
        let mut e = Expansion::default();
        let key = name.trim().to_ascii_lowercase();
        let Some(index) = self.particles.as_ref() else { return e };
        let Some(file) = index.systems.get(&key) else {
            e.missing.push((DepKind::Particle, name.trim().to_owned()));
            return e;
        };
        e.children.push((Req::File(DepKind::Particle), file.clone()));
        e
    }

    fn file(&self, vpath: &str, kind: DepKind) -> Expansion<'a> {
        let mut e = Expansion::default();
        let Some(source) = self.find(vpath) else {
            e.missing.push((kind, vpath.to_owned()));
            return e;
        };
        if kind == DepKind::Particle {
            let parsed = self.parsed(source, vpath, |bytes| match parse_pcf(bytes) {
                Some(p) => Parsed::Particles(Arc::new(p)),
                None => Parsed::Unreadable("not a binary particle file".to_owned()),
            });
            match parsed {
                Parsed::Particles(p) => e.children.extend(p.materials().into_iter().map(|m| (Req::Material, m))),
                Parsed::Unreadable(error) => e.warnings.push(DepWarning::Unreadable { vpath: vpath.to_owned(), error }),
                Parsed::Children(_) => {}
            }
            e.primary = Some(vpath.to_owned());
        }
        e.hits.push(self.hit(vpath.to_owned(), kind, source));
        e
    }
}

pub(crate) fn material_children(m: &hp_vmt::Material) -> Vec<(Req, String)> {
    let mut out = Vec::new();
    let mut visit = |key: &str, value: &str| {
        let optional = OPTIONAL_MATERIAL_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k));
        if key.eq_ignore_ascii_case("%tooltexture") {
            if !not_a_file(value) {
                out.push((Req::EditorTexture, hp_vmt::normalize_texture_path(value)));
            }
        } else if optional || MATERIAL_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k)) {
            let name = hp_vmt::normalize_material_name(value);
            if !name.is_empty() {
                out.push((if optional { Req::OptionalMaterial } else { Req::Material }, name));
            }
        } else if NON_FILE_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k)) || not_a_file(value) {
        } else if TEXTURE_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k)) {
            out.push((Req::Texture, hp_vmt::normalize_texture_path(value)));
        } else if key.starts_with('$') {
            out.push((Req::OptionalTexture, hp_vmt::normalize_texture_path(value)));
        }
    };
    for (k, v) in &m.params {
        visit(k, v);
    }
    let mut stack: Vec<&hp_kv::Block> = m.other_blocks.iter().collect();
    while let Some(block) = stack.pop() {
        if block.name.eq_ignore_ascii_case("proxies") {
            continue;
        }
        for (k, v) in &block.props {
            visit(k, v);
        }
        stack.extend(block.children.iter());
    }
    out.sort_unstable_by(|a, b| a.1.cmp(&b.1));
    out.dedup();
    out
}

fn root_request(root: &DepRoot) -> Option<Request> {
    let raw = root.name.trim();
    let reason = DepReason::Root(root.why.clone());
    let (req, name) = match root.kind {
        RootKind::Material => (Req::Material, hp_vmt::normalize_material_name(raw)),
        RootKind::Texture => (Req::Texture, hp_vmt::normalize_texture_path(raw)),
        RootKind::Sprite => (Req::Material, hp_vmt::normalize_material_name(raw)),
        RootKind::Model => {
            let mut p = normalize_path(raw).into_owned();
            if !p.starts_with("models/") {
                p.insert_str(0, "models/");
            }
            if !p.ends_with(".mdl") {
                return None;
            }
            (Req::Model, p)
        }
        RootKind::Sound => (Req::Sound, raw.to_owned()),
        RootKind::Particle => (Req::Particle, raw.to_owned()),
        RootKind::Soundscape => (Req::Soundscape, raw.to_owned()),
        RootKind::Skybox => (Req::Skybox, raw.to_ascii_lowercase()),
        RootKind::File => {
            let p = normalize_path(raw).into_owned();
            let kind = if p.ends_with(".pcf") { DepKind::Particle } else { DepKind::File };
            (Req::File(kind), p)
        }
    };
    (!name.is_empty()).then_some(Request { req, name, reason })
}

impl GameFs {
    fn build_indexes(&self, snapshot: &[Arc<Source>], cache: &DepsCache, map: &str, need: [bool; 3]) -> Indexes {
        let key = (self.generation(), map.to_owned());
        let mut idx = cache.indexes.lock();
        if idx.key.as_ref() != Some(&key) {
            *idx = Indexes { key: Some(key), ..Indexes::default() };
        }
        let find = |vpath: &str| snapshot.iter().find(|s| s.is_enabled() && s.contains(vpath)).map(|s| &**s);
        let read_text = |vpath: &str| find(vpath).and_then(|s| s.read(vpath)).map(crate::fs::decode_text);
        if need[0] && idx.sounds.is_none() {
            let mut files = read_text("scripts/game_sounds_manifest.txt")
                .map(|t| manifest_files(&t, &["precache_file", "preload_file"]))
                .unwrap_or_default();
            files.dedup();
            let texts: Vec<(String, Option<String>)> = files
                .into_par_iter()
                .map(|f| {
                    let t = read_text(&f);
                    (f, t)
                })
                .collect();
            let mut s = SoundIndex::default();
            for (f, t) in texts {
                if let Some(t) = t {
                    s.add_file(&f, &t, false);
                }
            }
            let level = format!("maps/{map}_level_sounds.txt");
            if let Some(t) = read_text(&level) {
                s.add_file(&level, &t, true);
            }
            idx.sounds = Some(Arc::new(s));
        }
        if need[1] && idx.scapes.is_none() {
            let files = read_text("scripts/soundscapes_manifest.txt")
                .map(|t| manifest_files(&t, &["file"]))
                .unwrap_or_default();
            let mut s = SoundscapeIndex::default();
            for f in files {
                if let Some(t) = read_text(&f) {
                    s.add_file(&f, &t, false);
                }
            }
            let own = format!("scripts/soundscapes_{map}.txt");
            if let Some(t) = read_text(&own) {
                s.add_file(&own, &t, true);
            }
            idx.scapes = Some(Arc::new(s));
        }
        if need[2] && idx.particles.is_none() {
            let base: Vec<String> = read_text("particles/particles_manifest.txt")
                .map(|t| manifest_files(&t, &["file"]))
                .unwrap_or_default()
                .into_iter()
                .map(|f| f.trim_start_matches('!').to_owned())
                .collect();
            let own: Vec<String> = read_text(&format!("maps/{map}_particles.txt"))
                .map(|t| manifest_files(&t, &["file"]))
                .unwrap_or_default()
                .into_iter()
                .map(|f| f.trim_start_matches('!').to_owned())
                .collect();
            let mut others: Vec<String> = Vec::new();
            for s in snapshot.iter().filter(|s| s.is_enabled() && s.origin.needs_packing()) {
                s.for_each_path("particles", Some("pcf"), &mut |p| others.push(p.to_owned()));
            }
            let mut order: Vec<String> = own.iter().chain(base.iter()).chain(others.iter()).cloned().collect();
            let mut seen = HashSet::new();
            order.retain(|f| seen.insert(f.clone()));
            let parsed: Vec<(String, Option<ParticleFile>)> = order
                .par_iter()
                .map(|f| {
                    let p = find(f).and_then(|s| s.read_cow(f).and_then(|b| parse_pcf(&b)));
                    (f.clone(), p)
                })
                .collect();
            let mut index = ParticleIndex { in_base_manifest: base.into_iter().collect(), ..ParticleIndex::default() };
            for (file, pcf) in parsed {
                let Some(pcf) = pcf else { continue };
                for (system, _) in &pcf.systems {
                    index.systems.entry(system.to_ascii_lowercase()).or_insert_with(|| file.clone());
                }
                index.files.insert(file, Arc::new(pcf));
            }
            idx.particles = Some(Arc::new(index));
        }
        Indexes {
            key: idx.key.clone(),
            sounds: idx.sounds.clone(),
            scapes: idx.scapes.clone(),
            particles: idx.particles.clone(),
        }
    }

    pub fn resolve_dependencies(
        &self,
        roots: &[DepRoot],
        opts: &ResolveOptions<'_>,
        cache: &DepsCache,
    ) -> DependencyGraph {
        let snapshot = self.snapshot();
        let map = opts.map_name.to_ascii_lowercase();
        let need = [
            roots.iter().any(|r| matches!(r.kind, RootKind::Sound | RootKind::Soundscape | RootKind::Model)),
            roots.iter().any(|r| r.kind == RootKind::Soundscape),
            roots.iter().any(|r| r.kind == RootKind::Particle),
        ];
        let indexes = self.build_indexes(&snapshot, cache, &map, need);
        let ctx = Ctx {
            snapshot: &snapshot,
            base: snapshot
                .iter()
                .filter(|s| s.is_enabled() && s.origin == SourceOrigin::BaseGame)
                .map(|s| &**s)
                .collect(),
            map: map.clone(),
            cache,
            sounds: indexes.sounds,
            scapes: indexes.scapes,
            particles: indexes.particles.clone(),
            editor_textures: opts.editor_textures,
        };

        let mut nodes: Vec<DepNode> = Vec::new();
        let mut node_at: HashMap<String, usize> = HashMap::new();
        let mut produced: HashMap<(Req, String), Vec<usize>> = HashMap::new();
        let mut missing: HashMap<(DepKind, String), Vec<DepReason>> = HashMap::new();
        let mut missing_of: HashMap<(Req, String), Vec<(DepKind, String)>> = HashMap::new();
        let mut warnings: Vec<DepWarning> = Vec::new();
        let mut targets_of: HashMap<(Req, String), Vec<String>> = HashMap::new();
        let mut links: HashSet<(String, String)> = HashSet::new();
        let parent_of = |reason: &DepReason| match reason {
            DepReason::Root(_) => Some(String::new()),
            DepReason::UsedBy(p) => Some(p.clone()),
            DepReason::Generated => None,
        };
        let mut level: Vec<Request> = roots.iter().filter_map(root_request).collect();
        let mut cancelled = false;
        let mut depth = 0;

        while !level.is_empty() {
            if opts.cancel.is_some_and(|c| c.load(Ordering::Relaxed)) || depth > 64 {
                cancelled = depth <= 64;
                break;
            }
            depth += 1;
            let mut fresh: Vec<Request> = Vec::new();
            let mut fresh_keys: HashSet<(Req, String)> = HashSet::new();
            let mut repeats: Vec<Request> = Vec::new();
            for r in level.drain(..) {
                let key = (r.req, r.name.clone());
                if produced.contains_key(&key) || !fresh_keys.insert(key) {
                    repeats.push(r);
                } else {
                    fresh.push(r);
                }
            }
            let expansions: Vec<Expansion<'_>> = fresh.par_iter().map(|r| ctx.expand(r)).collect();
            let mut next: Vec<Request> = Vec::new();
            for (r, e) in fresh.iter().zip(expansions) {
                let targets: Vec<String> = match &e.primary {
                    Some(p) => vec![p.clone()],
                    None => {
                        e.hits.iter().map(|h| h.vpath.clone()).chain(e.missing.iter().map(|m| m.1.clone())).collect()
                    }
                };
                if let Some(p) = &e.primary {
                    for child in e.hits.iter().map(|h| &h.vpath).chain(e.missing.iter().map(|m| &m.1)) {
                        if child != p {
                            links.insert((p.clone(), child.clone()));
                        }
                    }
                }
                if let Some(parent) = parent_of(&r.reason) {
                    links.extend(targets.iter().filter(|t| **t != parent).map(|t| (parent.clone(), t.clone())));
                }
                targets_of.insert((r.req, r.name.clone()), targets);
                let mut idxs = Vec::with_capacity(e.hits.len());
                for hit in e.hits {
                    let i = *node_at.entry(hit.vpath.clone()).or_insert_with(|| {
                        nodes.push(DepNode {
                            in_base: ctx.base.iter().any(|b| b.contains(&hit.vpath)),
                            vpath: hit.vpath,
                            kind: hit.kind,
                            source: Some(hit.source.id),
                            origin: Some(hit.source.origin),
                            source_label: hit.source.label.clone(),
                            workshop_id: workshop_id(hit.source),
                            size: hit.size,
                            reasons: Vec::new(),
                            reason_count: 0,
                            generated: None,
                        });
                        nodes.len() - 1
                    });
                    nodes[i].add_reason(&r.reason);
                    if hit.source.origin == SourceOrigin::Mounted {
                        warnings.push(DepWarning::MountedContent {
                            vpath: nodes[i].vpath.clone(),
                            source: hit.source.label.clone(),
                        });
                    }
                    idxs.push(i);
                }
                for m in &e.missing {
                    missing.entry(m.clone()).or_default().push(r.reason.clone());
                }
                warnings.extend(e.warnings);
                let child_reason = e.primary.map(DepReason::UsedBy).unwrap_or_else(|| r.reason.clone());
                for (req, name) in e.children {
                    next.push(Request { req, name, reason: child_reason.clone() });
                }
                missing_of.insert((r.req, r.name.clone()), e.missing);
                produced.insert((r.req, r.name.clone()), idxs);
            }
            for r in repeats {
                let key = (r.req, r.name.clone());
                if let (Some(parent), Some(targets)) = (parent_of(&r.reason), targets_of.get(&key)) {
                    links.extend(targets.iter().filter(|t| **t != parent).map(|t| (parent.clone(), t.clone())));
                }
                if let Some(idxs) = produced.get(&key) {
                    for &i in idxs {
                        nodes[i].add_reason(&r.reason);
                    }
                }
                if let Some(ms) = missing_of.get(&key) {
                    for m in ms {
                        let list = missing.entry(m.clone()).or_default();
                        if list.len() < MAX_REASONS && !list.contains(&r.reason) {
                            list.push(r.reason.clone());
                        }
                    }
                }
            }
            level = next;
        }

        if let Some(index) = &indexes.particles {
            let custom: Vec<String> = nodes
                .iter()
                .filter(|n| n.kind == DepKind::Particle && n.ships(PackPolicy { workshop: true, mounted: true }))
                .filter(|n| !index.in_base_manifest.contains(&n.vpath))
                .map(|n| n.vpath.clone())
                .collect();
            if !custom.is_empty() {
                let vpath = format!("maps/{map}_particles.txt");
                let existing = ctx
                    .find(&vpath)
                    .and_then(|s| s.read(&vpath))
                    .map(|b| manifest_files(&crate::fs::decode_text(b), &["file"]))
                    .unwrap_or_default();
                let mut files: Vec<String> = existing.iter().map(|f| f.trim_start_matches('!').to_owned()).collect();
                for c in custom {
                    if !files.contains(&c) {
                        files.push(c);
                    }
                }
                let mut text = String::from("particles_manifest\n{\n");
                for f in &files {
                    text.push_str(&format!("\t\"file\"\t\"!{f}\"\n"));
                }
                text.push_str("}\n");
                let bytes = Arc::new(text.into_bytes());
                let node = DepNode {
                    vpath: vpath.clone(),
                    kind: DepKind::ParticleManifest,
                    source: None,
                    origin: None,
                    source_label: String::new(),
                    workshop_id: None,
                    in_base: false,
                    size: bytes.len() as u64,
                    reasons: vec![DepReason::Generated],
                    reason_count: 1,
                    generated: Some(bytes),
                };
                match node_at.get(&vpath) {
                    Some(&i) => nodes[i] = node,
                    None => nodes.push(node),
                }
            }
        }

        nodes.sort_unstable_by(|a, b| a.vpath.cmp(&b.vpath));
        let mut missing: Vec<MissingAsset> =
            missing.into_iter().map(|((kind, name), reasons)| MissingAsset { kind, name, reasons }).collect();
        missing.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        let mut seen = HashSet::new();
        warnings.retain(|w| seen.insert(w.clone()));
        let mut links: Vec<(String, String)> = links.into_iter().collect();
        links.sort_unstable();
        DependencyGraph { nodes, missing, warnings, generation: self.generation(), cancelled, links }
    }

    pub fn materialize_dependency(&self, node: &DepNode, cache_dir: &Path) -> Result<PathBuf, FsError> {
        if let Some(bytes) = &node.generated {
            let dest = safe_join(cache_dir, &node.vpath).ok_or_else(|| FsError::InvalidPath(node.vpath.clone()))?;
            return match write_if_changed(dest, bytes)? {
                Outcome::Written(p) | Outcome::UpToDate(p) => Ok(p),
            };
        }
        let snapshot = self.snapshot();
        let source =
            snapshot.iter().find(|s| Some(s.id) == node.source).ok_or_else(|| FsError::NotFound(node.vpath.clone()))?;
        let payload = match &source.data {
            SourceData::Custom(index) if node.kind == DepKind::Texture && !source.contains(&node.vpath) => {
                let name = node.vpath.strip_prefix("materials/").and_then(|p| p.strip_suffix(".vtf")).unwrap_or("");
                let image = index.image_for_texture(name).ok_or_else(|| FsError::NotFound(node.vpath.clone()))?;
                Payload::EncodeImage(index, image)
            }
            _ => payload_of(source, &node.vpath).ok_or_else(|| FsError::NotFound(node.vpath.clone()))?,
        };
        match materialize(&node.vpath, &payload, cache_dir, false)? {
            Outcome::Written(p) | Outcome::UpToDate(p) => Ok(p),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{TempDir, vtf};
    use crate::{CustomDir, MountOptions};

    fn root(kind: RootKind, name: &str) -> DepRoot {
        DepRoot { kind, name: name.to_owned(), why: format!("test {name}") }
    }

    fn setup(tmp: &TempDir) -> GameFs {
        tmp.write(
            "gmod/garrysmod/gameinfo.txt",
            b"GameInfo { FileSystem { SearchPaths { game garrysmod game |all_source_engine_paths|sourceengine } } }",
        );
        tmp.write(
            "gmod/sourceengine/materials/base/wall.vmt",
            b"\"LightmappedGeneric\" { \"$basetexture\" \"base/wall\" }",
        );
        tmp.write("gmod/sourceengine/materials/base/wall.vtf", &vtf(4, 4, [1, 1, 1, 255]));
        tmp.write(
            "gmod/sourceengine/scripts/game_sounds_manifest.txt",
            b"game_sounds_manifest { \"precache_file\" \"scripts/game_sounds.txt\" }",
        );
        tmp.write(
            "gmod/sourceengine/scripts/game_sounds.txt",
            b"\"Base.Door\" { \"wave\" \"^doors/base.wav\" } \"Custom.Hum\" { \"wave\" \"custom/hum.wav\" }",
        );
        tmp.write("gmod/sourceengine/sound/doors/base.wav", b"RIFF");
        tmp.write(
            "gmod/sourceengine/scripts/soundscapes_manifest.txt",
            b"soundscapes_manifest { \"file\" \"scripts/soundscapes.txt\" }",
        );
        tmp.write(
            "gmod/sourceengine/scripts/soundscapes.txt",
            b"\"Generic\" { \"playlooping\" { \"wave\" \"doors/base.wav\" } }",
        );
        tmp.write(
            "gmod/sourceengine/particles/particles_manifest.txt",
            b"particles_manifest { \"file\" \"particles/base.pcf\" }",
        );
        tmp.write("gmod/sourceengine/particles/base.pcf", &pcf::tests::build_pcf(&[("base_fire", "base/wall")]));
        for face in SKY_FACES {
            tmp.write(
                &format!("gmod/sourceengine/materials/skybox/sky1{face}.vmt"),
                b"\"UnlitGeneric\" { \"$basetexture\" \"base/wall\" }",
            );
        }
        tmp.write(
            "gmod/garrysmod/materials/custom/brick.vmt",
            b"\"LightmappedGeneric\" { \"$basetexture\" \"custom/brick\" \"$bumpmap\" \"custom/brick_n\" \"$basetexturetransform\" \"center .5 .5 scale 1 1\" \"$detail\" \"custom/nope\" \"$envmap\" \"env_cubemap\" \"$surfaceprop\" \"brick\" \"$bottommaterial\" \"base/wall\" }",
        );
        tmp.write("gmod/garrysmod/materials/custom/brick.vtf", &vtf(4, 4, [9, 9, 9, 255]));
        tmp.write("gmod/garrysmod/materials/custom/brick_n.vtf", &vtf(4, 4, [9, 9, 9, 255]));
        tmp.write("gmod/garrysmod/sound/custom/hum.wav", b"RIFF1234");
        tmp.write(
            "gmod/garrysmod/scripts/soundscapes_gm_t.txt",
            b"\"Cave\" { \"playlooping\" { \"wave\" \"custom/hum.wav\" } \"playsoundscape\" { \"name\" \"Generic\" } }",
        );
        tmp.write("gmod/garrysmod/particles/custom.pcf", &pcf::tests::build_pcf(&[("custom_fire", "custom/brick")]));
        let opts = MountOptions { mount_workshop: false, mount_cfg: false, ..MountOptions::default() };
        GameFs::for_gmod(&tmp.path().join("gmod"), &opts).unwrap()
    }

    #[test]
    fn resolves_every_kind_with_reasons_and_missing() {
        let tmp = TempDir::new("deps");
        let fs = setup(&tmp);
        let roots = vec![
            root(RootKind::Material, "Custom/Brick"),
            root(RootKind::Material, "base/wall"),
            root(RootKind::Sound, "Base.Door"),
            root(RootKind::Sound, "#custom/hum.wav"),
            root(RootKind::Soundscape, "cave"),
            root(RootKind::Particle, "custom_fire"),
            root(RootKind::Particle, "base_fire"),
            root(RootKind::Skybox, "sky1"),
            root(RootKind::Model, "models/missing.mdl"),
            root(RootKind::Particle, "nope_fx"),
        ];
        let cache = DepsCache::new();
        let opts = ResolveOptions { map_name: "gm_t", ..ResolveOptions::default() };
        let g = fs.resolve_dependencies(&roots, &opts, &cache);
        let paths: Vec<&str> = g.nodes.iter().map(|n| n.vpath.as_str()).collect();
        for p in [
            "materials/custom/brick.vmt",
            "materials/custom/brick.vtf",
            "materials/custom/brick_n.vtf",
            "materials/base/wall.vmt",
            "materials/base/wall.vtf",
            "sound/doors/base.wav",
            "sound/custom/hum.wav",
            "scripts/soundscapes_gm_t.txt",
            "scripts/soundscapes.txt",
            "particles/custom.pcf",
            "particles/base.pcf",
            "maps/gm_t_particles.txt",
            "materials/skybox/sky1up.vmt",
        ] {
            assert!(paths.contains(&p), "{p} in {paths:?}");
        }
        let missing: Vec<&str> = g.missing.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(missing, ["materials/custom/nope.vtf", "models/missing.mdl", "nope_fx"]);
        assert_eq!(g.missing[0].reasons, [DepReason::UsedBy("materials/custom/brick.vmt".into())]);
        let wall = g.find("materials/base/wall.vmt").unwrap();
        assert!(wall.reason_count >= 2);
        assert!(!wall.ships(PackPolicy::default()));
        let brick = g.find("MATERIALS/custom/brick.vmt").unwrap();
        assert!(brick.ships(PackPolicy::default()));
        let manifest = g.find("maps/gm_t_particles.txt").unwrap();
        let text = String::from_utf8(manifest.generated.as_ref().unwrap().to_vec()).unwrap();
        assert!(text.contains("\"!particles/custom.pcf\""), "{text}");
        assert!(!text.contains("base.pcf"));
        assert!(!g.missing.iter().any(|m| m.name.contains("_hdr")));
        let out = tmp.mkdir("cache");
        for n in g.shipped(PackPolicy::default()) {
            let p = fs.materialize_dependency(n, &out).unwrap();
            assert!(p.is_file(), "{}", p.display());
        }
        assert!(!cache.is_empty());
        let again = fs.resolve_dependencies(&roots, &opts, &cache);
        assert_eq!(again.nodes.len(), g.nodes.len());
        assert_eq!(again.missing, g.missing);
    }

    #[test]
    fn custom_images_and_cancel() {
        let tmp = TempDir::new("deps-custom");
        tmp.write("art/Photo.png", &crate::test_util::png(8, 8, [1, 2, 3, 255]));
        let opts = MountOptions {
            mount_workshop: false,
            mount_cfg: false,
            custom_dirs: vec![CustomDir::with_prefix(tmp.path().join("art"), "mine")],
            ..MountOptions::default()
        };
        tmp.write("gmod/garrysmod/gameinfo.txt", b"GameInfo { FileSystem { SearchPaths { game garrysmod } } }");
        let fs = GameFs::for_gmod(&tmp.path().join("gmod"), &opts).unwrap();
        let cache = DepsCache::new();
        let g = fs.resolve_dependencies(
            &[root(RootKind::Material, "mine/photo")],
            &ResolveOptions { map_name: "x", ..ResolveOptions::default() },
            &cache,
        );
        assert!(g.missing.is_empty(), "{:?}", g.missing);
        let vtf_node = g.find("materials/mine/photo.vtf").unwrap();
        let out = tmp.mkdir("cache");
        let p = fs.materialize_dependency(vtf_node, &out).unwrap();
        assert!(p.starts_with(&out));
        assert!(hp_vtf::read_info(&std::fs::read(p).unwrap()).is_ok());
        let cancel = AtomicBool::new(true);
        let g = fs.resolve_dependencies(
            &[root(RootKind::Material, "mine/photo")],
            &ResolveOptions { map_name: "x", cancel: Some(&cancel), ..ResolveOptions::default() },
            &cache,
        );
        assert!(g.cancelled && g.nodes.is_empty());
    }
}
