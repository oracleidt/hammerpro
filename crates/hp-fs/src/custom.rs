use std::fs::File;
use std::io::BufReader;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

use ahash::{AHashMap, AHashSet};

use crate::dirindex::{ASSET_DIRS, PROFILE_GAME, PROFILE_MATERIALS, walk_files};
use crate::error::FsError;
use crate::path::{
    extension, is_safe_component, join_virtual, normalize_path, normalize_prefix, strip_extension, under_prefix,
};

pub const IMAGE_EXTENSIONS: &[&str] = &["png", "tga", "jpg", "jpeg", "bmp"];

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CustomDir {
    pub path: PathBuf,
    pub prefix: String,
    pub enabled: bool,
}

impl CustomDir {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let prefix = suggest_prefix(&path);
        Self { path, prefix, enabled: true }
    }

    pub fn with_prefix(path: impl Into<PathBuf>, prefix: impl Into<String>) -> Self {
        Self { path: path.into(), prefix: prefix.into(), enabled: true }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CustomMode {
    GameFolder,
    InsideMaterials,
    Prefixed,
    Detected,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CustomLayout {
    pub mode: CustomMode,
    pub prefix: String,
}

pub fn suggest_prefix(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    let safe = safe_component(&name);
    let name = if safe.is_empty() { "folder" } else { safe.as_str() };
    format!("custom/{name}")
}

fn push_safe_char(c: char, out: &mut String) {
    let lower = c.to_lowercase().next().unwrap_or(c);
    if lower.is_ascii_alphanumeric() || matches!(lower, '_' | '-' | '.') {
        out.push(lower);
    } else if let Some(folded) = fold_accent(lower) {
        out.push_str(folded);
    } else {
        out.push('_');
    }
}

fn safe_component(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        push_safe_char(c, &mut out);
    }
    out.trim_matches('.').to_owned()
}

pub fn safe_material_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for part in name.split(['/', '\\']) {
        let safe = safe_component(part);
        if safe.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&safe);
    }
    out
}

pub fn is_safe_material_name(name: &str) -> bool {
    safe_material_name(name) == name
}

pub fn game_texture_size(width: u32, height: u32) -> (u32, u32) {
    fn nearest_pow2(v: u32) -> u32 {
        if v >= hp_vtf::MAX_POW2_SIZE {
            return hp_vtf::MAX_POW2_SIZE;
        }
        if v <= 1 {
            return 1;
        }
        let up = v.next_power_of_two();
        let down = up >> 1;
        if v - down < up - v { down } else { up }
    }
    (nearest_pow2(width), nearest_pow2(height))
}

fn fold_accent(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
        'æ' => "ae",
        'ç' => "c",
        'è' | 'é' | 'ê' | 'ë' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ñ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => "o",
        'œ' => "oe",
        'ù' | 'ú' | 'û' | 'ü' => "u",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        _ => return None,
    })
}

pub fn custom_layout(dir: &CustomDir) -> CustomLayout {
    layout_with(dir, || walk_files(&dir.path, None, &is_material_file, Some(PROFILE_MATERIALS)).unwrap_or_default())
}

pub fn is_auto_prefix(dir: &CustomDir) -> bool {
    let prefix = normalize_prefix(&dir.prefix);
    prefix.is_empty() || prefix == suggest_prefix(&dir.path)
}

fn layout_with(dir: &CustomDir, list: impl FnOnce() -> Vec<String>) -> CustomLayout {
    if has_materials_subdir(&dir.path) {
        return CustomLayout { mode: CustomMode::GameFolder, prefix: String::new() };
    }
    if let Some(prefix) = prefix_inside_materials(&dir.path) {
        return CustomLayout { mode: CustomMode::InsideMaterials, prefix };
    }
    if is_auto_prefix(dir) {
        if let Some(prefix) = detect_prefix(&dir.path, &list()) {
            return CustomLayout { mode: CustomMode::Detected, prefix };
        }
        return CustomLayout { mode: CustomMode::Prefixed, prefix: suggest_prefix(&dir.path) };
    }
    CustomLayout { mode: CustomMode::Prefixed, prefix: normalize_prefix(&dir.prefix) }
}

const DETECT_MAX_VMTS: usize = 64;

fn prefix_for(texture: &str, stems: &AHashSet<String>) -> Option<String> {
    if stems.contains(texture) {
        return Some(String::new());
    }
    texture
        .match_indices('/')
        .find(|(i, _)| texture.get(i + 1..).is_some_and(|suffix| stems.contains(suffix)))
        .and_then(|(i, _)| texture.get(..i))
        .map(str::to_owned)
}

fn detect_prefix(root: &Path, files: &[String]) -> Option<String> {
    let stems: AHashSet<String> = files
        .iter()
        .filter(|f| {
            let ext = extension(f).to_ascii_lowercase();
            ext == "vtf" || IMAGE_EXTENSIONS.contains(&ext.as_str())
        })
        .map(|f| strip_extension(&normalize_path(f)).to_owned())
        .collect();
    if stems.is_empty() {
        return None;
    }
    let mut votes: AHashMap<String, usize> = AHashMap::new();
    let vmts = files.iter().filter(|f| extension(f).eq_ignore_ascii_case("vmt")).take(DETECT_MAX_VMTS);
    for rel in vmts {
        let Ok(bytes) = std::fs::read(root.join(rel)) else { continue };
        let Ok(material) = hp_vmt::parse_bytes(&bytes) else { continue };
        let (mut textures, _) = crate::export::references(&material);
        if let Some(base) = material.base_texture() {
            textures.insert(0, hp_vmt::normalize_texture_path(&base));
        }
        let vote = textures.iter().find_map(|t| prefix_for(t, &stems));
        if let Some(prefix) = vote.map(|p| normalize_prefix(&p))
            && (prefix.is_empty() || prefix.split('/').all(is_safe_component))
        {
            *votes.entry(prefix).or_default() += 1;
        }
    }
    votes
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.len().cmp(&a.0.len())).then_with(|| b.0.cmp(&a.0)))
        .map(|(prefix, _)| prefix)
}

fn has_materials_subdir(path: &Path) -> bool {
    let Ok(read) = std::fs::read_dir(path) else {
        return false;
    };
    read.flatten().any(|entry| {
        entry.file_name().to_str().is_some_and(|n| n.eq_ignore_ascii_case("materials"))
            && std::fs::metadata(entry.path()).is_ok_and(|m| m.is_dir())
    })
}

fn prefix_inside_materials(path: &Path) -> Option<String> {
    let parts: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let last = parts.iter().rposition(|p| p.eq_ignore_ascii_case("materials"))?;
    let rest = parts.get(last + 1..).unwrap_or_default().join("/");
    Some(normalize_prefix(&rest))
}

fn is_material_file(name: &str) -> bool {
    let ext = extension(name).to_ascii_lowercase();
    ext == "vmt" || ext == "vtf" || IMAGE_EXTENSIONS.contains(&ext.as_str())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlphaKind {
    Opaque,
    Binary,
    Blended,
}

impl AlphaKind {
    pub(crate) fn of_pixels(rgba: &[u8]) -> Self {
        let mut kind = Self::Opaque;
        for px in rgba.chunks_exact(4) {
            match px.get(3).copied().unwrap_or(255) {
                255 => {}
                0 => kind = Self::Binary,
                _ => return Self::Blended,
            }
        }
        kind
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ImageHeader {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) has_alpha_channel: bool,
}

fn read_image_header(path: &Path, ext: &str) -> Option<ImageHeader> {
    use image::ImageDecoder as _;
    let hinted = image::ImageFormat::from_extension(ext)?;
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let reader = if hinted == image::ImageFormat::Tga {
        image::ImageReader::with_format(reader, hinted)
    } else {
        let mut guessed = image::ImageReader::new(reader).with_guessed_format().ok()?;
        if guessed.format().is_none() {
            guessed.set_format(hinted);
        }
        guessed
    };
    let decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    Some(ImageHeader { width, height, has_alpha_channel: decoder.color_type().has_alpha() })
}

#[derive(Debug)]
pub(crate) struct ImageFile {
    pub(crate) rel: Box<str>,
    pub(crate) texture: Box<str>,
    pub(crate) ext: &'static str,
    header: OnceLock<Option<ImageHeader>>,
    alpha: OnceLock<AlphaKind>,
}

impl ImageFile {
    pub(crate) fn header(&self, root: &Path) -> Option<ImageHeader> {
        *self.header.get_or_init(|| read_image_header(&root.join(&*self.rel), self.ext))
    }

    pub(crate) fn game_size(&self, root: &Path) -> Option<(u32, u32)> {
        self.header(root).map(|h| game_texture_size(h.width, h.height))
    }

    pub(crate) fn alpha(&self, root: &Path) -> AlphaKind {
        *self.alpha.get_or_init(|| {
            if self.header(root).is_some_and(|h| !h.has_alpha_channel) {
                return AlphaKind::Opaque;
            }
            match self.decode_uncached(root) {
                Ok(img) => AlphaKind::of_pixels(&img.pixels),
                Err(_) => AlphaKind::Opaque,
            }
        })
    }

    pub(crate) fn decode(&self, root: &Path) -> Result<hp_vtf::RgbaImage, FsError> {
        let img = self.decode_uncached(root)?;
        if self.alpha.get().is_none() {
            let _ = self.alpha.set(AlphaKind::of_pixels(&img.pixels));
        }
        Ok(img)
    }

    fn decode_uncached(&self, root: &Path) -> Result<hp_vtf::RgbaImage, FsError> {
        let path = root.join(&*self.rel);
        let bytes = std::fs::read(&path).map_err(|e| FsError::io(&path, e))?;
        Ok(hp_vtf::load_image_file(&bytes, self.ext)?)
    }
}

#[derive(Debug)]
enum Entry {
    File(Box<str>),
    Synth(u32),
}

#[derive(Debug)]
pub(crate) enum CustomHit<'a> {
    File(PathBuf),
    Synth(&'a ImageFile),
}

#[derive(Debug)]
pub(crate) struct CustomIndex {
    dir: CustomDir,
    layout: CustomLayout,
    files: AHashMap<Box<str>, Entry>,
    legacy: AHashMap<Box<str>, Entry>,
    images: Vec<ImageFile>,
    image_by_texture: AHashMap<Box<str>, u32>,
}

impl CustomIndex {
    pub(crate) fn build(dir: &CustomDir) -> Result<Self, FsError> {
        if !dir.path.is_dir() {
            return Err(FsError::NotADirectory(dir.path.clone()));
        }
        let mut files: AHashMap<Box<str>, Entry> = AHashMap::new();
        let layout = if has_materials_subdir(&dir.path) {
            for rel in walk_files(&dir.path, Some(ASSET_DIRS), &|_| true, Some(PROFILE_GAME))? {
                let key: Box<str> = normalize_path(&rel).into();
                files.entry(key).or_insert_with(|| Entry::File(rel.into_boxed_str()));
            }
            CustomLayout { mode: CustomMode::GameFolder, prefix: String::new() }
        } else {
            let listed = walk_files(&dir.path, None, &is_material_file, Some(PROFILE_MATERIALS))?;
            let layout = layout_with(dir, || listed.clone());
            let base = if layout.prefix.is_empty() {
                "materials".to_owned()
            } else {
                join_virtual("materials", &layout.prefix)
            };
            for rel in listed {
                let key: Box<str> = join_virtual(&base, &normalize_path(&rel)).into();
                files.entry(key).or_insert_with(|| Entry::File(rel.into_boxed_str()));
            }
            layout
        };

        let mut found: Vec<(String, usize, Box<str>, String)> = files
            .iter()
            .filter_map(|(vpath, entry)| {
                let Entry::File(rel) = entry else { return None };
                let rest = vpath.strip_prefix("materials/")?;
                let ext = extension(rest);
                let rank = IMAGE_EXTENSIONS.iter().position(|e| *e == ext)?;
                let original = strip_extension(rest);
                Some((safe_material_name(original), rank, rel.clone(), original.to_owned()))
            })
            .filter(|(safe, ..)| !safe.is_empty())
            .collect();
        found.sort_unstable();
        let mut images = Vec::new();
        let mut image_by_texture = AHashMap::new();
        let mut originals: Vec<(u32, String)> = Vec::new();
        for (texture, rank, rel, original) in found {
            if image_by_texture.contains_key(texture.as_str()) {
                continue;
            }
            let Ok(index) = u32::try_from(images.len()) else { break };
            let ext = IMAGE_EXTENSIONS.get(rank).copied().unwrap_or("png");
            let texture: Box<str> = texture.into();
            image_by_texture.insert(texture.clone(), index);
            if *texture != *original {
                image_by_texture.entry(original.clone().into_boxed_str()).or_insert(index);
                let alias = format!("materials/{texture}.{ext}");
                files.entry(alias.into_boxed_str()).or_insert_with(|| Entry::File(rel.clone()));
                originals.push((index, original));
            }
            images.push(ImageFile { rel, texture, ext, header: OnceLock::new(), alpha: OnceLock::new() });
        }
        let has_real_vmt: AHashSet<u32> = originals
            .iter()
            .filter(|(_, original)| files.contains_key(format!("materials/{original}.vmt").as_str()))
            .map(|(i, _)| *i)
            .collect();
        for (index, image) in images.iter().enumerate() {
            let vmt = format!("materials/{}.vmt", image.texture);
            if let Ok(index) = u32::try_from(index)
                && !has_real_vmt.contains(&index)
                && !files.contains_key(vmt.as_str())
            {
                files.insert(vmt.into(), Entry::Synth(index));
            }
        }
        let mut legacy: AHashMap<Box<str>, Entry> = AHashMap::new();
        for (index, original) in originals {
            let vmt = format!("materials/{original}.vmt");
            if !files.contains_key(vmt.as_str()) {
                legacy.insert(vmt.into_boxed_str(), Entry::Synth(index));
            }
        }
        Ok(Self { dir: dir.clone(), layout, files, legacy, images, image_by_texture })
    }

    pub(crate) fn dir(&self) -> &CustomDir {
        &self.dir
    }

    pub(crate) fn layout(&self) -> &CustomLayout {
        &self.layout
    }

    pub(crate) fn root(&self) -> &Path {
        &self.dir.path
    }

    pub(crate) fn len(&self) -> usize {
        self.files.len()
    }

    #[inline]
    pub(crate) fn contains(&self, vpath: &str) -> bool {
        self.files.contains_key(vpath) || (!self.legacy.is_empty() && self.legacy.contains_key(vpath))
    }

    pub(crate) fn lookup(&self, vpath: &str) -> Option<CustomHit<'_>> {
        match self.files.get(vpath).or_else(|| self.legacy.get(vpath))? {
            Entry::File(rel) => Some(CustomHit::File(self.dir.path.join(&**rel))),
            Entry::Synth(i) => self.images.get(*i as usize).map(CustomHit::Synth),
        }
    }

    pub(crate) fn vmt_files(&self) -> impl Iterator<Item = &str> {
        self.files.iter().filter_map(|(vpath, entry)| match entry {
            Entry::File(_) if extension(vpath) == "vmt" => Some(&**vpath),
            _ => None,
        })
    }

    pub(crate) fn unsafe_names(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .files
            .iter()
            .filter_map(|(vpath, entry)| {
                let Entry::File(rel) = entry else { return None };
                let ext = extension(vpath);
                let name = strip_extension(vpath.strip_prefix("materials/")?);
                ((ext == "vmt" || ext == "vtf") && !is_safe_material_name(name)).then(|| rel.to_string())
            })
            .collect();
        out.sort_unstable();
        out
    }

    pub(crate) fn read(&self, vpath: &str) -> Option<Vec<u8>> {
        match self.lookup(vpath)? {
            CustomHit::File(path) => std::fs::read(path).ok(),
            CustomHit::Synth(image) => Some(self.synth_vmt(image).into_bytes()),
        }
    }

    pub(crate) fn image_for_texture(&self, texture: &str) -> Option<&ImageFile> {
        let i = *self.image_by_texture.get(texture)?;
        self.images.get(i as usize)
    }

    pub(crate) fn synth_vmt(&self, image: &ImageFile) -> String {
        let alpha = image.alpha(self.root());
        let opts = hp_vmt::ImageMaterialOptions {
            alphatest: alpha == AlphaKind::Binary,
            translucent: alpha == AlphaKind::Blended,
            ..hp_vmt::ImageMaterialOptions::default()
        };
        let file_name = image.rel.rsplit('/').next().unwrap_or(&image.rel);
        let mut text = format!("// Generated by Hammer Pro from {file_name}\n");
        text.push_str(&hp_vmt::write(&hp_vmt::for_image(&image.texture, &opts)));
        text
    }

    pub(crate) fn for_each_path<'a>(&'a self, prefix: &str, ext: Option<&str>, f: &mut dyn FnMut(&'a str)) {
        for key in self.files.keys() {
            if under_prefix(key, prefix) && ext.is_none_or(|e| extension(key) == e) {
                f(key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{TempDir, png};

    #[test]
    fn prefix_suggestions() {
        assert_eq!(suggest_prefix(Path::new("C:/Art/My Bricks")), "custom/my_bricks");
        assert_eq!(suggest_prefix(Path::new("/x/Textures Été")), "custom/textures_ete");
        assert_eq!(suggest_prefix(Path::new("/x/a\"b{c}")), "custom/a_b_c_");
        assert_eq!(suggest_prefix(Path::new("/x/...")), "custom/folder");
        assert_eq!(suggest_prefix(Path::new("/")), "custom/folder");
        assert_eq!(suggest_prefix(Path::new("/x/Cœur")), "custom/coeur");
    }

    #[test]
    fn layout_rules() {
        let tmp = TempDir::new("layout");
        let game = tmp.mkdir("pack");
        tmp.mkdir("pack/Materials/x");
        let inside = tmp.mkdir("art/materials/Custom/Bricks");
        let materials_root = tmp.path().join("art/materials");
        let plain = tmp.mkdir("My Textures");

        let l = custom_layout(&CustomDir::new(&game));
        assert_eq!(l.mode, CustomMode::GameFolder);
        assert_eq!(l.prefix, "");

        let l = custom_layout(&CustomDir::with_prefix(&inside, "ignored"));
        assert_eq!(l.mode, CustomMode::InsideMaterials);
        assert_eq!(l.prefix, "custom/bricks");

        let l = custom_layout(&CustomDir::new(&materials_root));
        assert_eq!(l.mode, CustomMode::InsideMaterials);
        assert_eq!(l.prefix, "");
        tmp.write("art/materials/Custom/Bricks/Red.vmt", b"x");
        let root_index = CustomIndex::build(&CustomDir::new(&materials_root)).unwrap();
        assert!(root_index.contains("materials/custom/bricks/red.vmt"));

        let l = custom_layout(&CustomDir::with_prefix(&plain, "Mine\\Walls/"));
        assert_eq!(l.mode, CustomMode::Prefixed);
        assert_eq!(l.prefix, "mine/walls");

        let l = custom_layout(&CustomDir::with_prefix(&plain, ""));
        assert_eq!(l.prefix, "custom/my_textures");
    }

    #[test]
    fn prefixed_folder_serves_materials_and_virtual_images() {
        let tmp = TempDir::new("custom-prefixed");
        tmp.write("Walls/Red.vmt", b"\"LightmappedGeneric\" { \"$basetexture\" \"mine/walls/red\" }");
        tmp.write("Walls/Red.vtf", b"VTF\0");
        tmp.write("Walls/Red.png", &png(4, 4, [255, 0, 0, 255]));
        tmp.write("Floor.PNG", &png(4, 4, [0, 255, 0, 255]));
        tmp.write("Fence.png", &png(2, 1, [0, 0, 0, 0]));
        tmp.write("Glass.png", &png(2, 1, [0, 0, 0, 100]));
        tmp.write("Floor.jpg", b"not used, png wins");
        tmp.write("notes.txt", b"ignored");
        let index = CustomIndex::build(&CustomDir::with_prefix(tmp.path(), "mine")).unwrap();
        assert!(index.contains("materials/mine/walls/red.vmt"));
        assert!(index.contains("materials/mine/walls/red.vtf"));
        assert!(index.contains("materials/mine/floor.png"));
        assert!(index.contains("materials/mine/floor.vmt"));
        assert!(!index.contains("materials/mine/notes.txt"));

        let red = index.read("materials/mine/walls/red.vmt").unwrap();
        assert!(red.starts_with(b"\"LightmappedGeneric\""));

        let floor = String::from_utf8(index.read("materials/mine/floor.vmt").unwrap()).unwrap();
        let mat = hp_vmt::parse(&floor).unwrap();
        assert_eq!(mat.base_texture().as_deref(), Some("mine/floor"));
        assert!(!mat.translucent() && !mat.alpha_test());
        assert_eq!(index.image_for_texture("mine/floor").map(|i| i.ext), Some("png"));

        let fence = hp_vmt::parse_bytes(&index.read("materials/mine/fence.vmt").unwrap()).unwrap();
        assert!(fence.alpha_test());
        let glass = hp_vmt::parse_bytes(&index.read("materials/mine/glass.vmt").unwrap()).unwrap();
        assert!(glass.translucent());

        let mut vmts = Vec::new();
        index.for_each_path("materials", Some("vmt"), &mut |p| vmts.push(p.to_owned()));
        vmts.sort();
        assert_eq!(
            vmts,
            [
                "materials/mine/fence.vmt",
                "materials/mine/floor.vmt",
                "materials/mine/glass.vmt",
                "materials/mine/walls/red.vmt"
            ]
        );
    }

    #[test]
    fn game_folder_mode_mounts_like_a_game_dir() {
        let tmp = TempDir::new("custom-game");
        tmp.write("materials/Pack/Stone.png", &png(8, 8, [9, 9, 9, 255]));
        tmp.write("models/pack/rock.mdl", b"IDST");
        tmp.write("lua/x.lua", b"");
        let index = CustomIndex::build(&CustomDir::new(tmp.path())).unwrap();
        assert_eq!(index.layout().mode, CustomMode::GameFolder);
        assert!(index.contains("materials/pack/stone.vmt"));
        assert!(index.contains("models/pack/rock.mdl"));
        assert!(!index.contains("lua/x.lua"));
        let header = index.image_for_texture("pack/stone").unwrap().header(index.root()).unwrap();
        assert_eq!((header.width, header.height), (8, 8));
    }

    #[test]
    fn safe_names() {
        assert_eq!(safe_material_name("Textures Été/Mur Brique Été"), "textures_ete/mur_brique_ete");
        assert_eq!(safe_material_name("a//b/./c/"), "a/b/c");
        assert_eq!(safe_material_name("../x\"y"), "x_y");
        assert!(is_safe_material_name("custom/brick_01-a.b"));
        assert!(!is_safe_material_name("custom/brick 01"));
        assert!(!is_safe_material_name("custom/Brick"));
        assert_eq!(game_texture_size(0, 1), (1, 1));
        assert_eq!(game_texture_size(4032, 3024), (4096, 2048));
        assert_eq!(game_texture_size(9000, 3), (hp_vtf::MAX_POW2_SIZE, 4));
    }

    #[test]
    fn ready_made_pack_is_mounted_where_its_vmts_expect() {
        let tmp = TempDir::new("custom-pack");
        let pack = tmp.mkdir("mypack");
        tmp.write(
            "mypack/brick01.vmt",
            b"LightmappedGeneric { $basetexture \"mypack/brick01\" $bumpmap mypack/brick01_n }",
        );
        tmp.write("mypack/brick01.vtf", b"VTF\0");
        tmp.write("mypack/brick01_n.vtf", b"VTF\0");
        tmp.write("mypack/sub/tile.vmt", b"LightmappedGeneric { $basetexture \"MyPack\\Sub\\Tile.vtf\" }");
        tmp.write("mypack/sub/tile.vtf", b"VTF\0");
        tmp.write("mypack/stock.vmt", b"LightmappedGeneric { $basetexture concrete/concretefloor001a }");
        let index = CustomIndex::build(&CustomDir::new(&pack)).unwrap();
        assert_eq!(index.layout(), &CustomLayout { mode: CustomMode::Detected, prefix: "mypack".into() });
        assert!(index.contains("materials/mypack/brick01.vmt"));
        assert!(index.contains("materials/mypack/brick01.vtf"));
        assert!(index.contains("materials/mypack/sub/tile.vtf"));
        assert_eq!(custom_layout(&CustomDir::with_prefix(&pack, "")).prefix, "mypack");

        let explicit = CustomIndex::build(&CustomDir::with_prefix(&pack, "other/place")).unwrap();
        assert_eq!(explicit.layout(), &CustomLayout { mode: CustomMode::Prefixed, prefix: "other/place".into() });

        let parent = CustomIndex::build(&CustomDir::new(tmp.path())).unwrap();
        assert_eq!(parent.layout(), &CustomLayout { mode: CustomMode::Detected, prefix: String::new() });
        assert!(parent.contains("materials/mypack/brick01.vtf"));

        let images = tmp.mkdir("Some Images");
        tmp.write("Some Images/a.png", &png(2, 2, [1, 1, 1, 255]));
        tmp.write("Some Images/b.vmt", b"LightmappedGeneric { $basetexture elsewhere/b }");
        let l = CustomIndex::build(&CustomDir::new(&images)).unwrap();
        assert_eq!(l.layout(), &CustomLayout { mode: CustomMode::Prefixed, prefix: "custom/some_images".into() });
    }

    #[test]
    fn detection_ignores_unsafe_prefixes() {
        let tmp = TempDir::new("custom-pack-unsafe");
        let pack = tmp.mkdir("p");
        tmp.write("p/a.vmt", b"LightmappedGeneric { $basetexture \"../../a\" }");
        tmp.write("p/a.vtf", b"VTF\0");
        let index = CustomIndex::build(&CustomDir::new(&pack)).unwrap();
        assert_eq!(index.layout().mode, CustomMode::Prefixed);
        assert_eq!(index.layout().prefix, "custom/p");
    }

    #[test]
    fn image_names_are_made_safe_and_old_names_still_resolve() {
        let tmp = TempDir::new("custom-safe-names");
        let dir = tmp.mkdir("Textures Été");
        tmp.write("Textures Été/Mur Brique Été.png", &png(4, 4, [200, 0, 0, 255]));
        tmp.write("Textures Été/Sous Dossier/Bois.png", &png(4, 4, [0, 200, 0, 255]));
        tmp.write("Textures Été/ok.png", &png(4, 4, [0, 0, 200, 255]));
        tmp.write(
            "Textures Été/Pierre Grise.vmt",
            b"LightmappedGeneric { $basetexture \"custom/textures_ete/pierre grise\" }",
        );
        tmp.write("Textures Été/Pierre Grise.png", &png(4, 4, [9, 9, 9, 255]));
        let index = CustomIndex::build(&CustomDir::new(&dir)).unwrap();
        assert_eq!(index.layout().prefix, "custom/textures_ete");
        let mut vmts = Vec::new();
        index.for_each_path("materials", Some("vmt"), &mut |p| vmts.push(p.to_owned()));
        vmts.sort();
        assert_eq!(
            vmts,
            [
                "materials/custom/textures_ete/mur_brique_ete.vmt",
                "materials/custom/textures_ete/ok.vmt",
                "materials/custom/textures_ete/pierre grise.vmt",
                "materials/custom/textures_ete/sous_dossier/bois.vmt",
            ]
        );
        let synth =
            hp_vmt::parse_bytes(&index.read("materials/custom/textures_ete/mur_brique_ete.vmt").unwrap()).unwrap();
        assert_eq!(synth.base_texture().as_deref(), Some("custom/textures_ete/mur_brique_ete"));
        assert!(index.image_for_texture("custom/textures_ete/mur_brique_ete").is_some());
        assert!(index.contains("materials/custom/textures_ete/mur brique Été.vmt"));
        assert!(index.image_for_texture("custom/textures_ete/mur brique Été").is_some());
        assert!(index.image_for_texture("custom/textures_ete/pierre grise").is_some());
        assert!(matches!(
            index.lookup("materials/custom/textures_ete/mur_brique_ete.png"),
            Some(CustomHit::File(p)) if p.ends_with("Mur Brique Été.png")
        ));
        assert_eq!(index.unsafe_names(), ["Pierre Grise.vmt"]);
    }

    #[test]
    fn missing_folder_is_an_error() {
        let tmp = TempDir::new("custom-missing");
        let err = CustomIndex::build(&CustomDir::new(tmp.path().join("nope"))).unwrap_err();
        assert!(matches!(err, FsError::NotADirectory(_)));
    }

    #[test]
    fn alpha_classification() {
        assert_eq!(AlphaKind::of_pixels(&[1, 2, 3, 255, 4, 5, 6, 255]), AlphaKind::Opaque);
        assert_eq!(AlphaKind::of_pixels(&[1, 2, 3, 0, 4, 5, 6, 255]), AlphaKind::Binary);
        assert_eq!(AlphaKind::of_pixels(&[1, 2, 3, 0, 4, 5, 6, 7]), AlphaKind::Blended);
    }
}
