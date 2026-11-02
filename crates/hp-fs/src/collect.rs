use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rayon::prelude::*;

use crate::deps::{DepKind, DepNode, DependencyGraph, Req, material_children, parse_pcf};
use crate::error::FsError;
use crate::export::{Payload, encode_image, payload_of, write_atomic};
use crate::fs::GameFs;
use crate::path::{extension, safe_join, strip_extension};
use crate::source::SourceData;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExistingFiles {
    #[default]
    Skip,
    Replace,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CollectOutcome {
    Copied,
    Replaced,
    Skipped,
    Same,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CollectedFile {
    pub vpath: String,
    pub outcome: CollectOutcome,
    pub bytes: u64,
}

#[derive(Clone, Copy, Default)]
pub struct CollectOptions<'a> {
    pub existing: ExistingFiles,
    pub cancel: Option<&'a AtomicBool>,
    pub progress: Option<&'a (dyn Fn(usize, usize) + Sync)>,
}

impl std::fmt::Debug for CollectOptions<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CollectOptions").field("existing", &self.existing).finish_non_exhaustive()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CollectRun {
    pub files: Vec<CollectedFile>,
    pub cancelled: bool,
}

impl CollectRun {
    pub fn count(&self, outcome: &CollectOutcome) -> usize {
        let same_kind = |o: &CollectOutcome| std::mem::discriminant(o) == std::mem::discriminant(outcome);
        self.files.iter().filter(|f| same_kind(&f.outcome)).count()
    }

    pub fn bytes_written(&self) -> u64 {
        self.files
            .iter()
            .filter(|f| matches!(f.outcome, CollectOutcome::Copied | CollectOutcome::Replaced))
            .map(|f| f.bytes)
            .sum()
    }
}

enum Content {
    Disk(PathBuf),
    Bytes(Vec<u8>),
}

impl GameFs {
    fn node_content(&self, node: &DepNode) -> Result<Content, FsError> {
        if let Some(bytes) = &node.generated {
            return Ok(Content::Bytes(bytes.to_vec()));
        }
        let snapshot = self.snapshot();
        let source =
            snapshot.iter().find(|s| Some(s.id) == node.source).ok_or_else(|| FsError::NotFound(node.vpath.clone()))?;
        if let SourceData::Custom(index) = &source.data
            && node.kind == DepKind::Texture
            && !source.contains(&node.vpath)
        {
            let name = node.vpath.strip_prefix("materials/").and_then(|p| p.strip_suffix(".vtf")).unwrap_or("");
            let image = index.image_for_texture(name).ok_or_else(|| FsError::NotFound(node.vpath.clone()))?;
            return Ok(Content::Bytes(encode_image(index, image)?));
        }
        match payload_of(source, &node.vpath).ok_or_else(|| FsError::NotFound(node.vpath.clone()))? {
            Payload::Disk(p) => Ok(Content::Disk(p)),
            Payload::Archive(s) => s
                .read_cow(&node.vpath)
                .map(|b| Content::Bytes(b.into_owned()))
                .ok_or(FsError::NotFound(node.vpath.clone())),
            Payload::SynthVmt(index, image) => Ok(Content::Bytes(index.synth_vmt(image).into_bytes())),
            Payload::EncodeImage(index, image) => Ok(Content::Bytes(encode_image(index, image)?)),
        }
    }

    fn collect_one(&self, node: &DepNode, out: &Path, existing: ExistingFiles) -> CollectedFile {
        let result = (|| -> Result<(CollectOutcome, u64), FsError> {
            let dest = safe_join(out, &node.vpath).ok_or_else(|| FsError::InvalidPath(node.vpath.clone()))?;
            let content = self.node_content(node)?;
            let present = dest.is_file();
            if present
                && let Content::Disk(src) = &content
                && let (Ok(a), Ok(b)) = (std::fs::canonicalize(src), std::fs::canonicalize(&dest))
                && a == b
            {
                let len = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
                return Ok((CollectOutcome::Same, len));
            }
            let bytes = match content {
                Content::Disk(src) => std::fs::read(&src).map_err(|e| FsError::io(&src, e))?,
                Content::Bytes(b) => b,
            };
            let len = bytes.len() as u64;
            if present {
                let same = std::fs::metadata(&dest).is_ok_and(|m| m.len() == len)
                    && std::fs::read(&dest).is_ok_and(|old| old == bytes);
                if same {
                    return Ok((CollectOutcome::Same, len));
                }
                if existing == ExistingFiles::Skip {
                    return Ok((CollectOutcome::Skipped, len));
                }
            }
            write_atomic(&dest, &bytes)?;
            Ok((if present { CollectOutcome::Replaced } else { CollectOutcome::Copied }, len))
        })();
        let (outcome, bytes) = result.unwrap_or_else(|e| (CollectOutcome::Failed(e.to_string()), 0));
        CollectedFile { vpath: node.vpath.clone(), outcome, bytes }
    }

    pub fn collect_dependencies(&self, nodes: &[&DepNode], out: &Path, opts: &CollectOptions<'_>) -> CollectRun {
        let total = nodes.len();
        let done = AtomicUsize::new(0);
        let stopped = || opts.cancel.is_some_and(|c| c.load(Ordering::Relaxed));
        let mut files: Vec<CollectedFile> = nodes
            .par_iter()
            .filter_map(|n| {
                if stopped() {
                    return None;
                }
                let file = self.collect_one(n, out, opts.existing);
                let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                if let Some(progress) = opts.progress {
                    progress(d, total);
                }
                Some(file)
            })
            .collect();
        files.sort_unstable_by(|a, b| a.vpath.cmp(&b.vpath));
        CollectRun { cancelled: files.len() < total, files }
    }
}

pub const CONTENT_FOLDERS: &[&str] = &["materials", "models", "sound", "particles"];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnusedFile {
    pub path: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnusedReport {
    pub root: PathBuf,
    pub folders: Vec<String>,
    pub scanned: usize,
    pub used: usize,
    pub unused: Vec<UnusedFile>,
    pub unused_bytes: u64,
    pub cancelled: bool,
}

fn content_root(folder: &Path) -> Result<(PathBuf, Vec<(String, PathBuf)>), FsError> {
    if !folder.is_dir() {
        return Err(FsError::NotADirectory(folder.to_path_buf()));
    }
    let subfolders = |root: &Path| -> Vec<(String, PathBuf)> {
        let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                CONTENT_FOLDERS.contains(&name.as_str()).then(|| (name, e.path()))
            })
            .collect();
        out.sort();
        out
    };
    let own = subfolders(folder);
    if !own.is_empty() {
        return Ok((folder.to_path_buf(), own));
    }
    let name = folder.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match folder.parent() {
        Some(parent) if CONTENT_FOLDERS.contains(&name.as_str()) => {
            Ok((parent.to_path_buf(), vec![(name, folder.to_path_buf())]))
        }
        _ => Err(FsError::NoContentFolders(folder.to_path_buf())),
    }
}

fn walk(dir: &Path, prefix: &str, cancel: &dyn Fn() -> bool, out: &mut Vec<(String, u64)>) {
    let mut stack = vec![(dir.to_path_buf(), prefix.to_owned())];
    while let Some((d, rel)) = stack.pop() {
        if cancel() {
            return;
        }
        for entry in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(kind) = entry.file_type() else { continue };
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("{rel}/{name}");
            if kind.is_dir() {
                stack.push((entry.path(), path));
            } else if kind.is_file() {
                out.push((path, entry.metadata().map(|m| m.len()).unwrap_or(0)));
            }
        }
    }
}

fn needs_of(vpath: &str, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    match extension(vpath) {
        "vmt" => {
            if let Ok(m) = hp_vmt::parse_bytes(bytes) {
                for (req, name) in material_children(&m) {
                    match req {
                        Req::Material | Req::OptionalMaterial => out.push(format!("materials/{name}.vmt")),
                        Req::Texture | Req::OptionalTexture | Req::EditorTexture => {
                            out.push(format!("materials/{name}.vtf"));
                        }
                        _ => {}
                    }
                }
            }
        }
        "mdl" => {
            if let Ok(deps) = hp_mdl::read_dependencies(bytes) {
                for candidates in deps.material_candidates() {
                    out.extend(candidates.into_iter().map(|c| format!("materials/{c}.vmt")));
                }
                out.extend(deps.include_models);
            }
        }
        "phy" => out.extend(hp_mdl::gib_models(bytes)),
        "pcf" => {
            if let Some(p) = parse_pcf(bytes) {
                out.extend(p.materials().into_iter().map(|m| format!("materials/{m}.vmt")));
            }
        }
        _ => {}
    }
    out
}

pub fn unused_files(
    folder: &Path,
    graph: &DependencyGraph,
    cancel: Option<&AtomicBool>,
) -> Result<UnusedReport, FsError> {
    let stopped = || cancel.is_some_and(|c| c.load(Ordering::Relaxed));
    let (root, folders) = content_root(folder)?;
    let mut files: Vec<(String, u64)> = Vec::new();
    for (name, dir) in &folders {
        let on_disk = dir.file_name().map_or_else(|| name.clone(), |n| n.to_string_lossy().into_owned());
        walk(dir, &on_disk, &stopped, &mut files);
    }
    files.sort_unstable_by_key(|(p, _)| p.to_ascii_lowercase());
    let present: HashMap<String, usize> =
        files.iter().enumerate().map(|(i, (p, _))| (p.to_ascii_lowercase(), i)).collect();
    let mut parts_of: HashMap<String, Vec<usize>> = HashMap::new();
    for (lower, &i) in &present {
        if lower.starts_with("models/") {
            let file = lower.rsplit('/').next().unwrap_or(lower);
            let dir_len = lower.len() - file.len();
            let stem = file.split('.').next().unwrap_or(file);
            parts_of.entry(format!("{}{stem}", &lower[..dir_len])).or_default().push(i);
        }
    }
    let mut used: HashSet<String> = graph.nodes.iter().map(|n| n.vpath.clone()).collect();
    used.extend(graph.missing.iter().map(|m| m.name.to_ascii_lowercase()));
    let mut queue: Vec<String> = present.keys().filter(|p| used.contains(*p)).cloned().collect();
    let mut cancelled = false;
    while let Some(vpath) = queue.pop() {
        if stopped() {
            cancelled = true;
            break;
        }
        let mut needs = Vec::new();
        if matches!(extension(&vpath), "vmt" | "mdl" | "phy" | "pcf")
            && let Some(bytes) = present.get(&vpath).and_then(|&i| std::fs::read(root.join(&files[i].0)).ok())
        {
            needs = needs_of(&vpath, &bytes);
        }
        if extension(&vpath) == "mdl" {
            let stem = strip_extension(&vpath);
            needs.extend(parts_of.get(stem).into_iter().flatten().map(|&i| files[i].0.to_ascii_lowercase()));
        }
        for need in needs {
            let need = crate::path::normalize_path(&need).into_owned();
            if present.contains_key(&need) && used.insert(need.clone()) {
                queue.push(need);
            }
        }
    }
    let mut report = UnusedReport {
        root,
        folders: folders.into_iter().map(|(n, _)| n).collect(),
        scanned: files.len(),
        cancelled,
        ..UnusedReport::default()
    };
    for (path, bytes) in files {
        if used.contains(&path.to_ascii_lowercase()) {
            report.used += 1;
        } else {
            report.unused_bytes += bytes;
            report.unused.push(UnusedFile { path, bytes });
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MountOptions;
    use crate::deps::{DepRoot, DepsCache, ResolveOptions, RootKind};
    use crate::test_util::{TempDir, vtf};

    fn vpk(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tree = Vec::new();
        for (path, data) in files {
            let (dir, file) = path.rsplit_once('/').unwrap();
            let (name, ext) = file.rsplit_once('.').unwrap();
            for part in [ext, dir, name] {
                tree.extend_from_slice(part.as_bytes());
                tree.push(0);
            }
            tree.extend_from_slice(&0u32.to_le_bytes());
            tree.extend_from_slice(&(data.len() as u16).to_le_bytes());
            tree.extend_from_slice(&0x7FFFu16.to_le_bytes());
            tree.extend_from_slice(&0u32.to_le_bytes());
            tree.extend_from_slice(&0u32.to_le_bytes());
            tree.extend_from_slice(&0xFFFFu16.to_le_bytes());
            tree.extend_from_slice(data);
            tree.extend_from_slice(&[0, 0]);
        }
        tree.push(0);
        let mut out = Vec::new();
        out.extend_from_slice(&0x55AA_1234u32.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&(tree.len() as u32).to_le_bytes());
        out.extend_from_slice(&tree);
        out
    }

    fn setup(tmp: &TempDir) -> GameFs {
        tmp.write("gmod/garrysmod/gameinfo.txt", b"GameInfo { FileSystem { SearchPaths { game garrysmod } } }");
        tmp.write(
            "gmod/sourceengine/base_dir.vpk",
            &vpk(&[
                ("materials/base/wall.vmt", b"\"LightmappedGeneric\" { \"$basetexture\" \"base/wall\" }"),
                ("materials/base/wall.vtf", &vtf(4, 4, [1, 1, 1, 255])),
            ]),
        );
        let mut gma = hp_gma::GmaWriter::new(hp_gma::AddonJson { title: "Pack".into(), ..Default::default() });
        gma.add(
            "materials/pack/rock.vmt",
            hp_gma::GmaSource::Bytes(b"\"VertexLitGeneric\" { \"$basetexture\" \"pack/rock\" }".to_vec()),
        )
        .unwrap();
        gma.add("materials/pack/rock.vtf", hp_gma::GmaSource::Bytes(vtf(4, 4, [5, 5, 5, 255]))).unwrap();
        std::fs::create_dir_all(tmp.path().join("gmod/garrysmod/addons")).unwrap();
        gma.write(&tmp.path().join("gmod/garrysmod/addons/pack.gma")).unwrap();
        tmp.write(
            "gmod/garrysmod/materials/custom/brick.vmt",
            b"\"LightmappedGeneric\" { \"$basetexture\" \"custom/brick\" \"%tooltexture\" \"custom/brick_tool\" }",
        );
        tmp.write("gmod/garrysmod/materials/custom/brick.vtf", &vtf(4, 4, [9, 9, 9, 255]));
        tmp.write("gmod/garrysmod/materials/custom/brick_tool.vtf", &vtf(4, 4, [8, 8, 8, 255]));
        let opts = MountOptions { mount_workshop: false, mount_cfg: false, ..MountOptions::default() };
        GameFs::for_gmod(&tmp.path().join("gmod"), &opts).unwrap()
    }

    fn graph(fs: &GameFs, editor: bool) -> DependencyGraph {
        let roots: Vec<DepRoot> = ["custom/brick", "base/wall", "pack/rock", "nope/missing"]
            .iter()
            .map(|n| DepRoot { kind: RootKind::Material, name: (*n).to_owned(), why: "faces".into() })
            .collect();
        let opts = ResolveOptions { map_name: "gm_t", editor_textures: editor, ..ResolveOptions::default() };
        fs.resolve_dependencies(&roots, &opts, &DepsCache::new())
    }

    #[test]
    fn tool_textures_and_links() {
        let tmp = TempDir::new("collect-links");
        let fs = setup(&tmp);
        assert!(graph(&fs, false).find("materials/custom/brick_tool.vtf").is_none());
        let g = graph(&fs, true);
        assert!(g.find("materials/custom/brick_tool.vtf").is_some());
        let top: Vec<&str> = g.children_of("").collect();
        assert_eq!(
            top,
            [
                "materials/base/wall.vmt",
                "materials/custom/brick.vmt",
                "materials/nope/missing.vmt",
                "materials/pack/rock.vmt"
            ]
        );
        let brick: Vec<&str> = g.children_of("materials/custom/brick.vmt").collect();
        assert_eq!(brick, ["materials/custom/brick.vtf", "materials/custom/brick_tool.vtf"]);
    }

    #[test]
    fn copies_extracts_and_never_overwrites_silently() {
        let tmp = TempDir::new("collect-copy");
        let fs = setup(&tmp);
        let g = graph(&fs, true);
        let out = tmp.mkdir("out");
        let nodes: Vec<&DepNode> = g.nodes.iter().collect();
        let calls = AtomicUsize::new(0);
        let progress = |_: usize, _: usize| {
            calls.fetch_add(1, Ordering::Relaxed);
        };
        let opts = CollectOptions { progress: Some(&progress), ..CollectOptions::default() };
        let run = fs.collect_dependencies(&nodes, &out, &opts);
        assert!(!run.cancelled);
        assert_eq!(run.count(&CollectOutcome::Copied), nodes.len(), "{run:?}");
        assert_eq!(calls.load(Ordering::Relaxed), nodes.len());
        for (vpath, origin) in [
            ("materials/custom/brick.vtf", "gmod/garrysmod/materials/custom/brick.vtf"),
            ("materials/custom/brick_tool.vtf", "gmod/garrysmod/materials/custom/brick_tool.vtf"),
        ] {
            assert_eq!(std::fs::read(out.join(vpath)).unwrap(), std::fs::read(tmp.path().join(origin)).unwrap());
        }
        assert_eq!(std::fs::read(out.join("materials/base/wall.vtf")).unwrap(), vtf(4, 4, [1, 1, 1, 255]));
        assert_eq!(std::fs::read(out.join("materials/pack/rock.vtf")).unwrap(), vtf(4, 4, [5, 5, 5, 255]));
        assert!(run.bytes_written() > 0);
        let again = fs.collect_dependencies(&nodes, &out, &CollectOptions::default());
        assert_eq!(again.count(&CollectOutcome::Same), nodes.len());
        std::fs::write(out.join("materials/base/wall.vmt"), b"mine").unwrap();
        let skip = fs.collect_dependencies(&nodes, &out, &CollectOptions::default());
        assert_eq!(skip.count(&CollectOutcome::Skipped), 1);
        assert_eq!(std::fs::read(out.join("materials/base/wall.vmt")).unwrap(), b"mine");
        let opts = CollectOptions { existing: ExistingFiles::Replace, ..CollectOptions::default() };
        let replace = fs.collect_dependencies(&nodes, &out, &opts);
        assert_eq!(replace.count(&CollectOutcome::Replaced), 1);
        assert_ne!(std::fs::read(out.join("materials/base/wall.vmt")).unwrap(), b"mine");
        let loose: Vec<&DepNode> = g.nodes.iter().filter(|n| n.vpath.starts_with("materials/custom/")).collect();
        let in_place = fs.collect_dependencies(&loose, &tmp.path().join("gmod/garrysmod"), &opts);
        assert_eq!(in_place.count(&CollectOutcome::Same), loose.len());
        let cancel = AtomicBool::new(true);
        let opts = CollectOptions { cancel: Some(&cancel), ..CollectOptions::default() };
        let cancelled = fs.collect_dependencies(&nodes, &out, &opts);
        assert!(cancelled.cancelled && cancelled.files.is_empty());
        let mut evil = g.nodes[0].clone();
        evil.vpath = "../evil.vmt".into();
        let r = fs.collect_dependencies(&[&evil], &out, &CollectOptions::default());
        assert!(matches!(r.files[0].outcome, CollectOutcome::Failed(_)));
        assert!(!tmp.path().join("evil.vmt").exists());
    }

    #[test]
    fn unused_files_of_a_content_folder() {
        let tmp = TempDir::new("collect-unused");
        let fs = setup(&tmp);
        let g = graph(&fs, false);
        tmp.write(
            "addon/materials/custom/brick.vmt",
            b"\"LightmappedGeneric\" { \"$basetexture\" \"custom/brick_old\" }",
        );
        tmp.write("addon/materials/custom/brick_old.vtf", b"x");
        tmp.write("addon/materials/nope/missing.vmt", b"\"UnlitGeneric\" { \"$basetexture\" \"nope/tex\" }");
        tmp.write("addon/materials/nope/tex.vtf", b"x");
        tmp.write("addon/materials/old/unused.vmt", b"\"UnlitGeneric\" { \"$basetexture\" \"old/unused\" }");
        tmp.write("addon/materials/old/unused.vtf", b"12345");
        tmp.write("addon/models/old/thing.mdl", b"IDST");
        tmp.write("addon/models/old/thing.vvd", b"x");
        tmp.write("addon/Sound/old/beep.wav", b"RIFF");
        tmp.write("addon/lua/autorun/x.lua", b"print(1)");
        let report = unused_files(&tmp.path().join("addon"), &g, None).unwrap();
        let unused: Vec<&str> = report.unused.iter().map(|u| u.path.as_str()).collect();
        assert_eq!(
            unused,
            [
                "materials/old/unused.vmt",
                "materials/old/unused.vtf",
                "models/old/thing.mdl",
                "models/old/thing.vvd",
                "Sound/old/beep.wav",
            ]
        );
        assert_eq!(report.scanned, 9);
        assert_eq!(report.used, 4);
        assert_eq!(report.folders, ["materials", "models", "sound"]);
        assert!(report.unused_bytes >= 5);
        let only = unused_files(&tmp.path().join("addon/models"), &g, None).unwrap();
        assert_eq!(only.root, tmp.path().join("addon"));
        assert_eq!(only.unused.len(), 2);
        assert!(matches!(unused_files(&tmp.path().join("addon/lua"), &g, None), Err(FsError::NoContentFolders(_))));
        assert!(matches!(unused_files(&tmp.path().join("nothing"), &g, None), Err(FsError::NotADirectory(_))));
    }
}
