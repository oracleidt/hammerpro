use std::borrow::Cow;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use hp_gma::GmaArchive;
use hp_vpk::VpkArchive;

use crate::custom::{CustomDir, CustomHit, CustomIndex};
use crate::dirindex::DirIndex;

#[derive(Clone, Debug)]
pub enum SourceKind {
    Dir,
    Vpk(Arc<VpkArchive>),
    Gma(Arc<GmaArchive>),
    Custom(CustomDir),
}

impl SourceKind {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Dir => "folder",
            Self::Vpk(_) => "vpk",
            Self::Gma(_) => "gma",
            Self::Custom(_) => "custom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum SourceOrigin {
    Custom,
    GameLoose,
    Addon,
    BaseGame,
    Mounted,
    Extra,
    Workshop,
    Download,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SourceMountKind {
    Dir,
    Vpk,
    Gma,
    Custom {
        prefix: String,
        #[serde(default = "custom_models_default")]
        include_models: bool,
    },
}

fn custom_models_default() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceMount {
    pub kind: SourceMountKind,
    pub label: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub origin: SourceOrigin,
}

impl SourceInfo {
    pub fn mount(&self) -> SourceMount {
        let kind = match &self.kind {
            SourceKind::Dir => SourceMountKind::Dir,
            SourceKind::Vpk(_) => SourceMountKind::Vpk,
            SourceKind::Gma(_) => SourceMountKind::Gma,
            SourceKind::Custom(dir) => {
                SourceMountKind::Custom { prefix: dir.prefix.clone(), include_models: dir.include_models }
            }
        };
        SourceMount {
            kind,
            label: self.label.clone(),
            path: self.path.clone(),
            enabled: self.enabled,
            origin: self.origin,
        }
    }
}

impl SourceOrigin {
    pub fn needs_packing(self) -> bool {
        matches!(self, Self::Custom | Self::GameLoose | Self::Addon | Self::Workshop | Self::Download)
    }
}

#[derive(Clone, Debug)]
pub struct SourceInfo {
    pub id: u32,
    pub kind: SourceKind,
    pub label: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub origin: SourceOrigin,
    pub file_count: usize,
}

#[derive(Debug)]
pub(crate) enum SourceData {
    Dir(Box<DirIndex>),
    Vpk(Arc<VpkArchive>),
    Gma(Arc<GmaArchive>),
    Custom(Box<CustomIndex>),
}

#[derive(Debug)]
pub(crate) struct Source {
    pub(crate) id: u32,
    pub(crate) label: String,
    pub(crate) path: PathBuf,
    pub(crate) origin: SourceOrigin,
    pub(crate) enabled: AtomicBool,
    pub(crate) data: SourceData,
}

pub(crate) type Bytes<'a> = Cow<'a, [u8]>;

impl Source {
    pub(crate) fn new(
        id: u32,
        label: String,
        path: PathBuf,
        origin: SourceOrigin,
        enabled: bool,
        data: SourceData,
    ) -> Self {
        Self { id, label, path, origin, enabled: AtomicBool::new(enabled), data }
    }

    #[inline]
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub(crate) fn info(&self) -> SourceInfo {
        let kind = match &self.data {
            SourceData::Dir(_) => SourceKind::Dir,
            SourceData::Vpk(v) => SourceKind::Vpk(Arc::clone(v)),
            SourceData::Gma(g) => SourceKind::Gma(Arc::clone(g)),
            SourceData::Custom(c) => SourceKind::Custom(c.dir().clone()),
        };
        SourceInfo {
            id: self.id,
            kind,
            label: self.label.clone(),
            path: self.path.clone(),
            enabled: self.is_enabled(),
            origin: self.origin,
            file_count: self.file_count(),
        }
    }

    pub(crate) fn file_count(&self) -> usize {
        match &self.data {
            SourceData::Dir(d) => d.len(),
            SourceData::Vpk(v) => v.len(),
            SourceData::Gma(g) => g.len(),
            SourceData::Custom(c) => c.len(),
        }
    }

    #[inline]
    pub(crate) fn contains(&self, vpath: &str) -> bool {
        match &self.data {
            SourceData::Dir(d) => d.contains(vpath),
            SourceData::Vpk(v) => v.contains(vpath),
            SourceData::Gma(g) => g.contains(vpath),
            SourceData::Custom(c) => c.contains(vpath),
        }
    }

    pub(crate) fn read(&self, vpath: &str) -> Option<Vec<u8>> {
        self.read_cow(vpath).map(Cow::into_owned)
    }

    pub(crate) fn read_cow(&self, vpath: &str) -> Option<Bytes<'_>> {
        match &self.data {
            SourceData::Dir(d) => read_disk(&d.disk_path(vpath)?, None).map(Cow::Owned),
            SourceData::Vpk(v) => v.read_cow(vpath).unwrap_or_else(|e| {
                log::warn!("{}: {e}", self.label);
                None
            }),
            SourceData::Gma(g) => g.read_cow(vpath).unwrap_or_else(|e| {
                log::warn!("{}: {e}", self.label);
                None
            }),
            SourceData::Custom(c) => c.read(vpath).map(Cow::Owned),
        }
    }

    pub(crate) fn read_prefix(&self, vpath: &str, len: usize) -> Option<Bytes<'_>> {
        match &self.data {
            SourceData::Dir(d) => read_disk(&d.disk_path(vpath)?, Some(len)).map(Cow::Owned),
            SourceData::Custom(c) => match c.lookup(vpath)? {
                CustomHit::File(path) => read_disk(&path, Some(len)).map(Cow::Owned),
                CustomHit::Synth(_) => c.read(vpath).map(Cow::Owned),
            },
            _ => self.read_cow(vpath),
        }
    }

    pub(crate) fn disk_path(&self, vpath: &str) -> Option<PathBuf> {
        match &self.data {
            SourceData::Dir(d) => d.disk_path(vpath),
            SourceData::Custom(c) => match c.lookup(vpath)? {
                CustomHit::File(path) => Some(path),
                CustomHit::Synth(_) => None,
            },
            SourceData::Vpk(_) | SourceData::Gma(_) => None,
        }
    }

    pub(crate) fn for_each_path<'a>(&'a self, prefix: &str, ext: Option<&str>, f: &mut dyn FnMut(&'a str)) {
        match &self.data {
            SourceData::Dir(d) => d.for_each_path(prefix, ext, f),
            SourceData::Vpk(v) => v.paths_with(prefix, ext).for_each(f),
            SourceData::Gma(g) => g.paths_with(prefix, ext).for_each(f),
            SourceData::Custom(c) => c.for_each_path(prefix, ext, f),
        }
    }

    pub(crate) fn watch_path(&self, vpath: &str) -> Option<PathBuf> {
        match &self.data {
            SourceData::Dir(d) => d.disk_path(vpath),
            SourceData::Custom(c) => match c.lookup(vpath)? {
                CustomHit::File(path) => Some(path),
                CustomHit::Synth(image) => Some(c.root().join(&*image.rel)),
            },
            SourceData::Vpk(_) | SourceData::Gma(_) => None,
        }
    }

    pub(crate) fn reindexed(&self) -> Option<Self> {
        let data = match &self.data {
            SourceData::Dir(d) => match DirIndex::build_game_dir(d.root()) {
                Ok(index) => SourceData::Dir(Box::new(index)),
                Err(e) => {
                    log::warn!("cannot refresh {}: {e}", self.label);
                    return None;
                }
            },
            SourceData::Custom(c) => match CustomIndex::build(c.dir()) {
                Ok(index) => SourceData::Custom(Box::new(index)),
                Err(e) => {
                    log::warn!("cannot refresh {}: {e}", self.label);
                    return None;
                }
            },
            SourceData::Vpk(_) | SourceData::Gma(_) => return None,
        };
        Some(Self::new(self.id, self.label.clone(), self.path.clone(), self.origin, self.is_enabled(), data))
    }
}

pub(crate) fn read_disk(path: &Path, limit: Option<usize>) -> Option<Vec<u8>> {
    let result = match limit {
        None => std::fs::read(path),
        Some(limit) => File::open(path).and_then(|file| {
            let mut out = Vec::with_capacity(limit.min(1 << 20));
            file.take(limit as u64).read_to_end(&mut out)?;
            Ok(out)
        }),
    };
    match result {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            log::debug!("cannot read {}: {e}", path.display());
            None
        }
    }
}
