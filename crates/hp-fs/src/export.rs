use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use hp_vmt::Material;
use rayon::prelude::*;

use crate::custom::{CustomHit, CustomIndex, ImageFile};
use crate::error::FsError;
use crate::fs::GameFs;
use crate::path::safe_join;
use crate::source::{Source, SourceData, SourceOrigin};
use crate::texture::{TextureHit, find_texture};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportFailure {
    pub path: String,
    pub error: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportReport {
    pub written: Vec<PathBuf>,
    pub up_to_date: Vec<PathBuf>,
    pub failed: Vec<ExportFailure>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResizedImage {
    pub texture: String,
    pub size: (u32, u32),
    pub game_size: (u32, u32),
}

const MATERIAL_KEYS: &[&str] = &["$bottommaterial", "$underwateroverlay", "$crackmaterial", "$fallbackmaterial"];

const NON_TEXTURE_KEYS: &[&str] = &["$surfaceprop", "$surfaceprop2", "%keywords", "$model"];

#[derive(Clone)]
pub(crate) enum Payload<'a> {
    Disk(PathBuf),
    Archive(&'a Source),
    SynthVmt(&'a CustomIndex, &'a ImageFile),
    EncodeImage(&'a CustomIndex, &'a ImageFile),
}

pub(crate) enum Outcome {
    Written(PathBuf),
    UpToDate(PathBuf),
}

pub(crate) fn payload_of<'a>(source: &'a Source, vpath: &str) -> Option<Payload<'a>> {
    match &source.data {
        SourceData::Dir(d) => d.disk_path(vpath).map(Payload::Disk),
        SourceData::Vpk(_) | SourceData::Gma(_) => Some(Payload::Archive(source)),
        SourceData::Custom(c) => match c.lookup(vpath)? {
            CustomHit::File(path) => Some(Payload::Disk(path)),
            CustomHit::Synth(image) => Some(Payload::SynthVmt(c, image)),
        },
    }
}

fn not_a_texture(value: &str) -> bool {
    let v = value.trim();
    v.is_empty()
        || v.starts_with(['[', '{', '$', '%', '-', '.'])
        || v.parse::<f64>().is_ok()
        || v.eq_ignore_ascii_case("env_cubemap")
        || v.to_ascii_lowercase().starts_with("_rt_")
}

pub(crate) fn references(mat: &Material) -> (Vec<String>, Vec<String>) {
    let mut textures = Vec::new();
    let mut materials = Vec::new();
    let mut visit = |key: &str, value: &str| {
        if MATERIAL_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k)) || key.eq_ignore_ascii_case("include") {
            let name = hp_vmt::normalize_material_name(value);
            if !name.is_empty() {
                materials.push(name);
            }
        } else if !NON_TEXTURE_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k)) && !not_a_texture(value) {
            textures.push(hp_vmt::normalize_texture_path(value));
        }
    };
    for (k, v) in &mat.params {
        visit(k, v);
    }
    let mut stack: Vec<&hp_kv::Block> = mat.other_blocks.iter().collect();
    while let Some(block) = stack.pop() {
        if block.name.eq_ignore_ascii_case("proxies") {
            continue;
        }
        for (k, v) in &block.props {
            visit(k, v);
        }
        stack.extend(block.children.iter());
    }
    textures.sort_unstable();
    textures.dedup();
    materials.sort_unstable();
    materials.dedup();
    (textures, materials)
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub(crate) fn write_atomic(dest: &Path, bytes: &[u8]) -> Result<(), FsError> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| FsError::io(parent, e))?;
    }
    let mut tmp_name = dest.as_os_str().to_owned();
    tmp_name.push(".hptmp");
    let tmp = PathBuf::from(tmp_name);
    std::fs::write(&tmp, bytes).map_err(|e| FsError::io(&tmp, e))?;
    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        FsError::io(dest, e)
    })
}

pub(crate) fn write_if_changed(dest: PathBuf, bytes: &[u8]) -> Result<Outcome, FsError> {
    let same_size = std::fs::metadata(&dest).is_ok_and(|m| m.len() == bytes.len() as u64);
    if same_size && std::fs::read(&dest).is_ok_and(|existing| existing == bytes) {
        return Ok(Outcome::UpToDate(dest));
    }
    write_atomic(&dest, bytes)?;
    Ok(Outcome::Written(dest))
}

pub(crate) fn encode_image(index: &CustomIndex, image: &ImageFile) -> Result<Vec<u8>, FsError> {
    let img = image.decode(index.root())?;
    Ok(hp_vtf::encode(&img, &hp_vtf::EncodeOptions::default())?)
}

pub(crate) fn materialize(
    vpath: &str,
    payload: &Payload<'_>,
    dest_root: &Path,
    copy_disk: bool,
) -> Result<Outcome, FsError> {
    if let Payload::Disk(path) = payload
        && !copy_disk
    {
        return Ok(Outcome::UpToDate(path.clone()));
    }
    let dest = safe_join(dest_root, vpath).ok_or_else(|| FsError::InvalidPath(vpath.to_owned()))?;
    match payload {
        Payload::Disk(src) => {
            let src_meta = std::fs::metadata(src).map_err(|e| FsError::io(src, e))?;
            if let Ok(dest_meta) = std::fs::metadata(&dest)
                && dest_meta.len() == src_meta.len()
                && let (Ok(d), Ok(s)) = (dest_meta.modified(), src_meta.modified())
                && d >= s
            {
                return Ok(Outcome::UpToDate(dest));
            }
            let bytes = std::fs::read(src).map_err(|e| FsError::io(src, e))?;
            write_atomic(&dest, &bytes)?;
            Ok(Outcome::Written(dest))
        }
        Payload::Archive(source) => {
            let bytes = source.read_cow(vpath).ok_or_else(|| FsError::NotFound(vpath.to_owned()))?;
            write_if_changed(dest, &bytes)
        }
        Payload::SynthVmt(index, image) => write_if_changed(dest, index.synth_vmt(image).as_bytes()),
        Payload::EncodeImage(index, image) => {
            let src = index.root().join(&*image.rel);
            if let (Some(d), Some(s)) = (mtime(&dest), mtime(&src))
                && d >= s
            {
                return Ok(Outcome::UpToDate(dest));
            }
            let vtf = encode_image(index, image)?;
            write_atomic(&dest, &vtf)?;
            Ok(Outcome::Written(dest))
        }
    }
}

struct Collector<'a> {
    snapshot: &'a [Arc<Source>],
    include: &'a (dyn Fn(&Source, &str) -> bool + Sync),
    jobs: BTreeMap<String, Payload<'a>>,
    seen_materials: HashSet<String>,
    resized: Vec<ResizedImage>,
    quiet: bool,
}

impl<'a> Collector<'a> {
    fn new(snapshot: &'a [Arc<Source>], include: &'a (dyn Fn(&Source, &str) -> bool + Sync)) -> Self {
        Self {
            snapshot,
            include,
            jobs: BTreeMap::new(),
            seen_materials: HashSet::new(),
            resized: Vec::new(),
            quiet: false,
        }
    }

    fn find(&self, vpath: &str) -> Option<&'a Source> {
        self.snapshot.iter().find(|s| s.is_enabled() && s.contains(vpath)).map(|s| &**s)
    }

    fn add_material(&mut self, name: &str) {
        let mut queue = VecDeque::from([hp_vmt::normalize_material_name(name)]);
        while let Some(name) = queue.pop_front() {
            if name.is_empty() || !self.seen_materials.insert(name.clone()) {
                continue;
            }
            let vpath = format!("materials/{name}.vmt");
            let Some(source) = self.find(&vpath) else { continue };
            if !(self.include)(source, &vpath) {
                continue;
            }
            let Some(payload) = payload_of(source, &vpath) else { continue };
            self.jobs.insert(vpath.clone(), payload);
            let Some(material) = source.read(&vpath).and_then(|b| hp_vmt::parse_bytes(&b).ok()) else {
                continue;
            };
            let (textures, materials) = references(&material);
            for texture in textures {
                self.add_texture(&texture);
            }
            queue.extend(materials);
        }
    }

    fn add_texture(&mut self, name: &str) {
        if name.is_empty() {
            return;
        }
        match find_texture(self.snapshot, name) {
            Some(TextureHit::Vtf { source, path }) => {
                if (self.include)(source, &path)
                    && let Some(payload) = payload_of(source, &path)
                {
                    self.jobs.insert(path, payload);
                }
            }
            Some(TextureHit::Image { source, index, image }) => {
                let path = format!("materials/{name}.vtf");
                if (self.include)(source, &path) {
                    if let Some(header) = image.header(index.root())
                        && let Some(game_size) = image.game_size(index.root())
                        && (header.width, header.height) != game_size
                        && !self.jobs.contains_key(&path)
                    {
                        if !self.quiet {
                            log::warn!(
                                "{name}: the {}x{} image is resized to {}x{} for the game (sides must be powers of two)",
                                header.width,
                                header.height,
                                game_size.0,
                                game_size.1
                            );
                        }
                        self.resized.push(ResizedImage {
                            texture: name.to_owned(),
                            size: (header.width, header.height),
                            game_size,
                        });
                    }
                    self.jobs.insert(path, Payload::EncodeImage(index, image));
                }
            }
            None => {}
        }
    }
}

impl GameFs {
    pub fn resized_custom_images(&self, used_materials: &[String]) -> Vec<ResizedImage> {
        let snapshot = self.snapshot();
        let include = |source: &Source, _: &str| source.origin == SourceOrigin::Custom;
        let mut collector = Collector::new(&snapshot, &include);
        collector.quiet = true;
        for name in used_materials {
            collector.add_material(name);
        }
        collector.resized
    }

    pub fn export_custom_materials(&self, used_materials: &[String], game_dir: &Path) -> Result<ExportReport, FsError> {
        if !game_dir.is_dir() {
            return Err(FsError::NotADirectory(game_dir.to_path_buf()));
        }
        let snapshot = self.snapshot();
        let include = |source: &Source, _: &str| source.origin == SourceOrigin::Custom;
        let mut collector = Collector::new(&snapshot, &include);
        for name in used_materials {
            collector.add_material(name);
        }
        let mut report = ExportReport::default();
        let results: Vec<(String, Result<Outcome, FsError>)> = collector
            .jobs
            .into_par_iter()
            .map(|(vpath, payload)| {
                let outcome = materialize(&vpath, &payload, game_dir, true);
                (vpath, outcome)
            })
            .collect();
        for (path, result) in results {
            match result {
                Ok(Outcome::Written(p)) => report.written.push(p),
                Ok(Outcome::UpToDate(p)) => report.up_to_date.push(p),
                Err(e) => report.failed.push(ExportFailure { path, error: e.to_string() }),
            }
        }
        Ok(report)
    }

    pub fn pack_list(
        &self,
        used_materials: &[String],
        used_models: &[String],
        cache_dir: &Path,
    ) -> Vec<(String, PathBuf)> {
        self.pack_list_with(used_materials, used_models, &[], "", cache_dir)
    }

    pub fn pack_list_with(
        &self,
        used_materials: &[String],
        used_models: &[String],
        other_roots: &[crate::deps::DepRoot],
        map_name: &str,
        cache_dir: &Path,
    ) -> Vec<(String, PathBuf)> {
        use crate::deps::{DepRoot, DepsCache, PackPolicy, ResolveOptions, RootKind};
        let root = |kind, name: &String| DepRoot { kind, name: name.clone(), why: String::new() };
        let roots: Vec<DepRoot> = used_materials
            .iter()
            .map(|m| root(RootKind::Material, m))
            .chain(used_models.iter().map(|m| root(RootKind::Model, m)))
            .chain(other_roots.iter().cloned())
            .collect();
        let opts = ResolveOptions { map_name, ..ResolveOptions::default() };
        let graph = self.resolve_dependencies(&roots, &opts, &DepsCache::new());
        let policy = PackPolicy { workshop: true, mounted: false };
        let nodes: Vec<&crate::deps::DepNode> = graph.shipped(policy).collect();
        nodes
            .into_par_iter()
            .filter_map(|node| match self.materialize_dependency(node, cache_dir) {
                Ok(path) => Some((node.vpath.clone(), path)),
                Err(e) => {
                    log::warn!("cannot pack {}: {e}", node.vpath);
                    None
                }
            })
            .collect()
    }

    pub fn export_custom_dependencies(
        &self,
        roots: &[crate::deps::DepRoot],
        map_name: &str,
        dest: &Path,
        cache: &crate::deps::DepsCache,
    ) -> ExportReport {
        use crate::deps::RootKind;
        let snapshot = self.snapshot();
        let custom: Vec<&Source> =
            snapshot.iter().filter(|s| s.is_enabled() && s.origin == SourceOrigin::Custom).map(|s| &**s).collect();
        let holds = |prefix: &str| {
            custom.iter().any(|s| {
                let mut found = false;
                s.for_each_path(prefix, None, &mut |_| found = true);
                found
            })
        };
        let sounds = holds("sound") || holds("scripts") || holds("maps");
        let mut systems: HashSet<String> = HashSet::new();
        for source in &custom {
            let mut files: Vec<&str> = Vec::new();
            source.for_each_path("particles", Some("pcf"), &mut |p| files.push(p));
            for file in files {
                if let Some(pcf) = source.read_cow(file).and_then(|b| crate::deps::parse_pcf(&b)) {
                    systems.extend(pcf.systems.iter().map(|(name, _)| name.to_ascii_lowercase()));
                }
            }
        }
        let custom_model = |name: &str| {
            let mut vpath = crate::path::normalize_path(name.trim()).into_owned();
            if !vpath.starts_with("models/") {
                vpath.insert_str(0, "models/");
            }
            custom.iter().any(|s| s.contains(&vpath))
        };
        let roots: Vec<crate::deps::DepRoot> = roots
            .iter()
            .filter(|r| match r.kind {
                RootKind::Model => custom_model(&r.name),
                RootKind::Sound | RootKind::Soundscape => sounds,
                RootKind::Particle => systems.contains(&r.name.trim().to_ascii_lowercase()),
                _ => true,
            })
            .cloned()
            .collect();
        if custom.is_empty() || roots.is_empty() {
            return ExportReport::default();
        }
        let opts = crate::deps::ResolveOptions { map_name, ..crate::deps::ResolveOptions::default() };
        let graph = self.resolve_dependencies(&roots, &opts, cache);
        let nodes: Vec<&crate::deps::DepNode> =
            graph.nodes.iter().filter(|n| n.generated.is_some() || n.origin == Some(SourceOrigin::Custom)).collect();
        let results: Vec<(String, Result<Outcome, FsError>)> =
            nodes.into_par_iter().map(|n| (n.vpath.clone(), self.write_dependency(n, dest, true))).collect();
        let mut report = ExportReport::default();
        for (path, result) in results {
            match result {
                Ok(Outcome::Written(p)) => report.written.push(p),
                Ok(Outcome::UpToDate(p)) => report.up_to_date.push(p),
                Err(e) => report.failed.push(ExportFailure { path, error: e.to_string() }),
            }
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{TempDir, png, vtf};
    use crate::{CustomDir, MountOptions};

    #[test]
    fn reference_scan() {
        let m = hp_vmt::parse(
            r#""LightmappedGeneric" {
                "$basetexture" "Custom/Brick.png" "$bumpmap" "custom/brick_n" "$envmap" "env_cubemap"
                "$color" "[1 1 1]" "$alpha" "0.5" "$surfaceprop" "concrete" "$envmapmask" "_rt_x"
                "$bottommaterial" "water/bottom"
                ">=dx90" { "$detail" "detail/noise" }
                "Proxies" { "AnimatedTexture" { "animatedtexturevar" "$basetexture" "animatedtextureframenumvar" "$frame" } }
            }"#,
        )
        .unwrap();
        let (textures, materials) = references(&m);
        assert_eq!(textures, ["custom/brick", "custom/brick_n", "detail/noise"]);
        assert_eq!(materials, ["water/bottom"]);
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
        tmp.write("gmod/sourceengine/models/base.mdl", b"IDST");
        tmp.write("gmod/garrysmod/materials/base/wall.vtf", &vtf(4, 4, [1, 1, 1, 255]));
        tmp.write(
            "gmod/garrysmod/materials/loose/new.vmt",
            b"\"LightmappedGeneric\" { \"$basetexture\" \"base/wall\" }",
        );
        tmp.write("gmod/garrysmod/addons/pack/models/props/box.mdl", b"IDST");
        tmp.write("gmod/garrysmod/addons/pack/models/props/box.vvd", b"IDSV");
        tmp.write("gmod/garrysmod/addons/pack/models/props/box.dx90.vtx", b"vtx");
        tmp.write("gmod/garrysmod/addons/pack/models/props/box.phy", b"phy");
        tmp.write("art/Brick.png", &png(8, 8, [150, 60, 40, 255]));
        tmp.write("art/Detail.png", &png(4, 4, [9, 9, 9, 255]));
        tmp.write(
            "art/Mixed.vmt",
            b"\"LightmappedGeneric\" { \"$basetexture\" \"base/wall\" \"$detail\" \"mine/detail\" }",
        );
        tmp.write("art/Ready.vmt", b"\"LightmappedGeneric\" { \"$basetexture\" \"mine/ready\" }");
        tmp.write("art/Ready.vtf", &vtf(4, 4, [5, 5, 5, 255]));
        let opts = MountOptions {
            mount_workshop: false,
            mount_cfg: false,
            custom_dirs: vec![CustomDir::with_prefix(tmp.path().join("art"), "mine")],
            ..MountOptions::default()
        };
        GameFs::for_gmod(&tmp.path().join("gmod"), &opts).unwrap()
    }

    #[test]
    fn exports_custom_materials_and_skips_up_to_date_files() {
        let tmp = TempDir::new("export");
        let fs = setup(&tmp);
        let out = tmp.mkdir("out");
        let used: Vec<String> = ["Mine/Brick", "mine/mixed", "mine/ready", "base/wall", "missing/x"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let report = fs.export_custom_materials(&used, &out).unwrap();
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        let mut written: Vec<String> =
            report.written.iter().map(|p| p.strip_prefix(&out).unwrap().to_string_lossy().replace('\\', "/")).collect();
        written.sort();
        assert_eq!(
            written,
            [
                "materials/mine/brick.vmt",
                "materials/mine/brick.vtf",
                "materials/mine/detail.vtf",
                "materials/mine/mixed.vmt",
                "materials/mine/ready.vmt",
                "materials/mine/ready.vtf",
            ]
        );
        let vtf_bytes = std::fs::read(out.join("materials/mine/brick.vtf")).unwrap();
        let info = hp_vtf::read_info(&vtf_bytes).unwrap();
        assert_eq!((info.width, info.height), (8, 8));
        assert!(info.mip_count > 1);
        let vmt = hp_vmt::parse_bytes(&std::fs::read(out.join("materials/mine/brick.vmt")).unwrap()).unwrap();
        assert_eq!(vmt.base_texture().as_deref(), Some("mine/brick"));
        assert!(!out.join("materials/base").exists());

        let again = fs.export_custom_materials(&used, &out).unwrap();
        assert!(again.written.is_empty(), "{:?}", again.written);
        assert_eq!(again.up_to_date.len(), 6);

        assert!(fs.export_custom_materials(&used, &tmp.path().join("nope")).is_err());
    }

    #[test]
    fn materialize_rejects_unsafe_names() {
        let tmp = TempDir::new("export-unsafe");
        let src = tmp.write("src.vmt", b"x");
        let out = tmp.mkdir("out");
        for bad in ["../evil.vmt", "materials/../../evil.vmt", "/abs.vmt", "materials/con.vmt", "c:/x.vmt"] {
            let result = materialize(bad, &Payload::Disk(src.clone()), &out, true);
            assert!(matches!(result, Err(FsError::InvalidPath(_))), "{bad}");
        }
        assert!(!tmp.path().join("evil.vmt").exists());
        assert!(matches!(materialize("materials/ok.vmt", &Payload::Disk(src), &out, true), Ok(Outcome::Written(_))));
        let fs = GameFs::empty();
        let report = fs.export_custom_materials(&["../../x/con".to_owned()], &out).unwrap();
        assert_eq!(report, ExportReport::default());
    }

    #[test]
    fn pack_list_includes_non_base_files_only() {
        let tmp = TempDir::new("pack");
        let fs = setup(&tmp);
        let cache = tmp.mkdir("cache");
        let materials: Vec<String> =
            ["mine/brick", "mine/mixed", "loose/new", "base/wall"].iter().map(|s| (*s).to_owned()).collect();
        let models: Vec<String> = ["models/props/box.mdl", "base.mdl"].iter().map(|s| (*s).to_owned()).collect();
        let list = fs.pack_list(&materials, &models, &cache);
        let paths: Vec<&str> = list.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(
            paths,
            [
                "materials/loose/new.vmt",
                "materials/mine/brick.vmt",
                "materials/mine/brick.vtf",
                "materials/mine/detail.vtf",
                "materials/mine/mixed.vmt",
                "models/props/box.dx90.vtx",
                "models/props/box.mdl",
                "models/props/box.phy",
                "models/props/box.vvd",
            ]
        );
        for (_, disk) in &list {
            assert!(disk.is_file(), "{}", disk.display());
        }
        let brick = list.iter().find(|(p, _)| p == "materials/mine/brick.vtf").unwrap();
        assert!(brick.1.starts_with(&cache));
        let mixed = list.iter().find(|(p, _)| p == "materials/mine/mixed.vmt").unwrap();
        assert!(mixed.1.starts_with(tmp.path().join("art")));
    }
}
