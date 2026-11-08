use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use hp_fs::AssetEntry;

use crate::material::{MaterialData, material_job};
use crate::models::{PreparedModel, model_job};
use crate::queue::{BC_YES, BoundsState, FsEnv, INFLIGHT_TEXTURE_BYTES, Shared, THUMB_STALE_FRAMES};
use crate::texture::{PreparedTexture, TexFailure, TexTarget, prepare_texture};
use crate::thumb_cache::ThumbDiskCache;

pub(crate) const THUMB_SIZE: u32 = 128;

pub(crate) enum Job {
    Material { name: String, fresh: bool, hdr: bool, mode_epoch: u64, env: FsEnv },
    Texture { name: String, target: TexTarget, env: FsEnv },
    Thumb { name: String, fresh: bool, last_seen: Arc<AtomicU64>, env: FsEnv },
    Model { path: String, env: FsEnv },
    ModelLods { key: String, env: FsEnv },
}

pub(crate) struct ThumbPixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub size: [u32; 2],
}

pub(crate) struct MountData {
    pub root: PathBuf,
    pub fs: hp_fs::GameFs,
    pub lists: AssetLists,
    pub rest: hp_fs::DeferredMount,
}

pub(crate) struct AssetLists {
    pub fs_gen: u64,
    pub materials: Arc<Vec<AssetEntry>>,
    pub models: Arc<Vec<AssetEntry>>,
    pub sounds: Arc<Vec<AssetEntry>>,
    pub labels: Vec<(u32, String)>,
}

impl AssetLists {
    pub(crate) fn build(fs: &hp_fs::GameFs) -> Self {
        let fs_gen = fs.generation();
        let sources = fs.sources();
        let editor_only: Vec<u32> =
            sources.iter().filter(|s| crate::mount::is_editor_only_source(&s.path)).map(|s| s.id).collect();
        let listed = |list: Arc<Vec<AssetEntry>>| {
            if list.iter().any(|e| editor_only.contains(&e.source)) {
                Arc::new(list.iter().filter(|e| !editor_only.contains(&e.source)).cloned().collect())
            } else {
                list
            }
        };
        let materials = listed(fs.materials());
        let models = listed(fs.models());
        let sounds = listed(fs.sounds());
        let labels = sources.into_iter().map(|s| (s.id, s.label)).collect();
        Self { fs_gen, materials, models, sounds, labels }
    }
}

pub(crate) enum MountOutcome {
    NotConfigured,
    Mounted(Box<MountData>),
    Failed(String),
}

pub(crate) enum JobResult {
    Material { name: String, epoch: u64, hdr: bool, mode_epoch: u64, data: MaterialData },
    Texture { name: String, target: TexTarget, epoch: u64, data: Option<PreparedTexture>, held: crate::queue::Held },
    Thumb { name: String, epoch: u64, data: Option<ThumbPixels> },
    ThumbDropped { name: String },
    Model { path: String, epoch: u64, data: Result<Arc<PreparedModel>, String> },
    ModelLods { key: String, epoch: u64, data: Result<Arc<crate::lods::PreparedLods>, String> },
    Mounted { ticket: u64, outcome: MountOutcome },
    MountedLate { ticket: u64, added: usize },
    FgdLoaded { ticket: u64, outcome: crate::mount::FgdOutcome },
    Lists { epoch: u64, lists: AssetLists },
    Refreshed { names: Vec<String> },
}

pub(crate) fn run(job: Job, shared: &Shared) -> Option<JobResult> {
    match job {
        Job::Material { name, fresh, hdr, mode_epoch, env } => {
            let data = material_job(&env.fs, &name, fresh, hdr);
            Some(JobResult::Material { name, epoch: env.epoch, hdr, mode_epoch, data })
        }
        Job::Texture { name, target, env } => {
            shared.inflight.wait_below(INFLIGHT_TEXTURE_BYTES, &shared.shutdown);
            let bc = shared.bc.load(Ordering::Relaxed) == BC_YES;
            let data = prepare_texture(&env.fs, &name, bc).map_err(|f| shared.report_texture(env.epoch, f)).ok();
            let bytes = data.as_ref().map_or(0, |p| p.upload.levels.iter().map(|l| l.len() as u64).sum());
            let held = shared.inflight.hold(bytes);
            Some(JobResult::Texture { name, target, epoch: env.epoch, data, held })
        }
        Job::Thumb { name, fresh, last_seen, env } => {
            let frame = shared.current_frame();
            if last_seen.load(Ordering::Relaxed).saturating_add(THUMB_STALE_FRAMES) < frame {
                return Some(JobResult::ThumbDropped { name });
            }
            let cache = shared.thumb_cache.read().clone();
            let key = cache.as_ref().and_then(|_| ThumbDiskCache::key(&env.fs, &name));
            if let (Some(cache), Some(key)) = (&cache, key)
                && !fresh
                && let Some(px) = cache.load(key)
            {
                return Some(JobResult::Thumb { name, epoch: env.epoch, data: Some(px) });
            }
            let start = std::time::Instant::now();
            let data = thumb_job(&env.fs, &name, fresh).map_err(|f| shared.report_texture(env.epoch, f)).ok();
            if let (Some(cache), Some(key), Some(px)) = (&cache, key, &data)
                && start.elapsed() >= crate::thumb_cache::MIN_COST
            {
                cache.store(key, px);
            }
            Some(JobResult::Thumb { name, epoch: env.epoch, data })
        }
        Job::Model { path, env } => {
            let data = model_job(&env.fs, &path).map(Arc::new);
            let state = match &data {
                Ok(m) => BoundsState::Ready { min: m.bounds.0, max: m.bounds.1 },
                Err(e) => {
                    log::debug!("model {path}: {e}");
                    BoundsState::Failed
                }
            };
            shared.bounds.write().insert(path.clone(), state);
            Some(JobResult::Model { path, epoch: env.epoch, data })
        }
        Job::ModelLods { key, env } => {
            let data = crate::lods::lod_job(&env.fs, &key).map(Arc::new);
            Some(JobResult::ModelLods { key, epoch: env.epoch, data })
        }
    }
}

pub(crate) fn thumb_job(fs: &hp_fs::GameFs, name: &str, fresh: bool) -> Result<ThumbPixels, TexFailure> {
    let mat = if fresh { crate::material::load_fresh(fs, name).map(Arc::new) } else { fs.material(name) };
    let mat = mat.ok_or(TexFailure::Missing)?.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
    let texture = mat.preview_texture().ok_or(TexFailure::Missing)?;
    let broken = |reason: String| TexFailure::Broken { texture: texture.clone(), reason };
    let mut tex = match fs.load_texture(&texture, Some(THUMB_SIZE)) {
        Ok(tex) => tex,
        Err(hp_fs::FsError::NotFound(_)) => return Err(TexFailure::Missing),
        Err(e) => return Err(broken(e.to_string())),
    };
    if mat.tool_kind(name).is_none() {
        let usage = mat.alpha_usage(&|t| fs.texture_alpha_flag(t).unwrap_or(false));
        let usage = crate::material::shown_usage(&mat, usage, Some(&texture));
        let table = crate::material::preview_alpha_table(&usage);
        for px in tex.rgba.chunks_exact_mut(4) {
            px[3] = table[usize::from(px[3])];
        }
    }
    let expected = (tex.width as usize).checked_mul(tex.height as usize).and_then(|n| n.checked_mul(4));
    if tex.width == 0 || tex.height == 0 || expected != Some(tex.rgba.len()) {
        return Err(broken(format!("bad pixel data for {}x{}", tex.width, tex.height)));
    }
    if mat.is_blend() && mat.tool_kind(name).is_none() && mat.base_texture().as_deref() == Some(&texture) {
        let count = if mat.shader_kind() == hp_vmt::ShaderKind::Lightmapped4WayBlend { 4 } else { 2 };
        let usage = mat.alpha_usage(&|t| fs.texture_alpha_flag(t).unwrap_or(false));
        let table = crate::material::preview_alpha_table(&usage);
        for layer in 2..=count {
            let Some(name) = mat.texture(&format!("$basetexture{layer}")) else { continue };
            let Ok(other) = fs.load_texture(&name, Some(THUMB_SIZE)) else { continue };
            blend_thumbnail_layer(&mut tex, &other, layer - 1, count, &table);
        }
    }
    Ok(ThumbPixels {
        width: tex.width,
        height: tex.height,
        rgba: tex.rgba,
        size: [tex.original_width, tex.original_height],
    })
}

fn blend_thumbnail_layer(
    base: &mut hp_fs::LoadedTexture,
    other: &hp_fs::LoadedTexture,
    layer: u32,
    count: u32,
    alpha: &[u8; 256],
) {
    if other.width == 0 || other.height == 0 || other.rgba.len() != other.width as usize * other.height as usize * 4 {
        return;
    }
    for y in 0..base.height {
        for x in 0..base.width {
            let (u, v) = ((x as f32 + 0.5) / base.width as f32, (y as f32 + 0.5) / base.height as f32);
            let weight = if count == 2 {
                ((u + v - 0.9) * 5.0).clamp(0.0, 1.0)
            } else {
                f32::from(u8::from(u >= 0.5)) + 2.0 * f32::from(u8::from(v >= 0.5))
            };
            let weight = if count == 2 { weight } else { f32::from(u8::from(weight as u32 == layer)) };
            if weight == 0.0 {
                continue;
            }
            let ox = (u64::from(x) * u64::from(other.width) / u64::from(base.width)) as usize;
            let oy = (u64::from(y) * u64::from(other.height) / u64::from(base.height)) as usize;
            let source = &other.rgba[(oy * other.width as usize + ox) * 4..][..4];
            let target = &mut base.rgba[(y as usize * base.width as usize + x as usize) * 4..][..4];
            for (i, value) in target.iter_mut().enumerate() {
                let incoming = if i == 3 { alpha[source[3] as usize] } else { source[i] };
                *value = (f32::from(*value) + (f32::from(incoming) - f32::from(*value)) * weight).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/scratch").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn vmt(texture: &str) -> String {
        format!("LightmappedGeneric {{ $basetexture \"{texture}\" }}")
    }

    #[test]
    fn broken_textures_are_reported_once_per_mount_missing_ones_never() {
        hp_app::register_translations();
        crate::register_translations();
        let dir = scratch("broken_textures");
        let folder = dir.join("bomb");
        let _ = std::fs::create_dir_all(&folder);
        let img = hp_vtf::RgbaImage::new(16, 16, [200u8, 100, 50, 255].repeat(256)).unwrap();
        let opts =
            hp_vtf::EncodeOptions { format: hp_vtf::EncodeFormat::Bgra8888, mipmaps: false, ..Default::default() };
        let good = hp_vtf::encode(&img, &opts).unwrap();
        let files: [(&str, &[u8]); 3] =
            [("good.vtf", &good), ("trunc.vtf", &good[..good.len() / 2]), ("zero.vtf", &[])];
        for (file, bytes) in files {
            std::fs::write(folder.join(file), bytes).unwrap();
        }
        for (name, texture) in
            [("good", "bomb/good"), ("trunc", "bomb/Trunc"), ("zero", "bomb/zero"), ("lost", "bomb/nowhere")]
        {
            std::fs::write(folder.join(format!("{name}.vmt")), vmt(texture)).unwrap();
        }
        let fs = hp_fs::GameFs::empty();
        fs.add_custom_dir(hp_fs::CustomDir::with_prefix(&folder, "bomb")).unwrap();
        let (tx, _rx) = crossbeam_channel::unbounded();
        let shared = Shared::new(tx);
        let env = |epoch| FsEnv { fs: fs.clone(), epoch };
        let texture = |name: &str, epoch| {
            let job = Job::Texture { name: name.to_owned(), target: TexTarget::Main, env: env(epoch) };
            match run(job, &shared) {
                Some(JobResult::Texture { data, .. }) => data.is_some(),
                _ => panic!("texture result expected"),
            }
        };
        let thumb = |name: &str, epoch| {
            let last_seen = Arc::new(AtomicU64::new(shared.current_frame()));
            let job = Job::Thumb { name: name.to_owned(), fresh: false, last_seen, env: env(epoch) };
            match run(job, &shared) {
                Some(JobResult::Thumb { data, .. }) => data.is_some(),
                _ => panic!("thumbnail result expected"),
            }
        };
        assert!(texture("bomb/good", 1));
        assert!(thumb("bomb/good", 1));
        assert_eq!(shared.broken_count(1), 0);
        assert!(!texture("bomb/nowhere", 1));
        assert!(!thumb("bomb/lost", 1));
        assert!(!thumb("bomb/no_such_material", 1));
        assert_eq!(shared.broken_count(1), 0);
        assert!(!texture("bomb/trunc", 1));
        assert!(!texture("bomb/zero", 1));
        assert_eq!(shared.broken_count(1), 2);
        assert!(!thumb("bomb/trunc", 1));
        assert!(!thumb("bomb/zero", 1));
        assert!(!texture("bomb/trunc", 1));
        assert_eq!(shared.broken_count(1), 2);
        assert!(!thumb("bomb/trunc", 2));
        assert_eq!(shared.broken_count(2), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failures_say_what_is_wrong() {
        let img = hp_vtf::RgbaImage::new(8, 8, [1u8, 2, 3, 255].repeat(64)).unwrap();
        let opts =
            hp_vtf::EncodeOptions { format: hp_vtf::EncodeFormat::Bgra8888, mipmaps: false, ..Default::default() };
        let good = hp_vtf::encode(&img, &opts).unwrap();
        let err = crate::texture::prepare_vtf(&good[..good.len() - 10], false).err().unwrap_or_default();
        assert!(err.contains("truncated"), "{err}");
        let err = crate::texture::prepare_vtf(b"nope", false).err().unwrap_or_default();
        assert!(!err.is_empty());
    }

    #[test]
    fn thumbnails_ignore_the_alpha_of_opaque_materials() {
        let dir = scratch("thumb_alpha");
        let mats = dir.join("g/materials/t");
        let _ = std::fs::create_dir_all(&mats);
        let img = hp_vtf::RgbaImage::new(8, 8, [200u8, 100, 50, 100].repeat(64)).unwrap();
        let vtf = hp_vtf::encode(&img, &hp_vtf::EncodeOptions { mipmaps: false, ..Default::default() }).unwrap();
        std::fs::write(mats.join("tex.vtf"), vtf).unwrap();
        for (name, params) in [
            ("opaque", ""),
            ("mask", "$basealphaenvmapmask 1 $translucent 1"),
            ("glass", "$translucent 1"),
            ("cutout", "$alphatest 1"),
            ("kept", "$alphatest 1 $alphatestreference .35 $translucent 1"),
            ("faded", "$translucent 1 $alpha .5"),
        ] {
            let vmt = format!("LightmappedGeneric {{ $basetexture t/tex {params} }}");
            std::fs::write(mats.join(format!("{name}.vmt")), vmt).unwrap();
        }
        let fs = hp_fs::GameFs::empty();
        fs.add_game_dir(&dir.join("g")).unwrap();
        let alpha = |name: &str| thumb_job(&fs, name, false).ok().map(|t| t.rgba[3]);
        assert_eq!(alpha("t/opaque"), Some(255));
        assert_eq!(alpha("t/mask"), Some(255));
        assert_eq!(alpha("t/glass"), Some(100));
        assert_eq!(alpha("t/cutout"), Some(0), "100 is below the 0.7 alpha test reference");
        assert_eq!(alpha("t/kept"), Some(255), "above the 0.35 reference: kept, not blended");
        assert_eq!(alpha("t/faded"), Some(50), "$alpha .5 times the texture alpha");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn blend_material_swatches_show_each_layer_and_keep_base_dimensions() {
        let dir = scratch("blend_swatches");
        let mats = dir.join("g/materials/t");
        std::fs::create_dir_all(&mats).unwrap();
        let colors = [[240, 20, 10, 0], [10, 220, 20, 0], [20, 10, 230, 0], [220, 200, 10, 0]];
        for (i, color) in colors.iter().enumerate() {
            let (w, h) = if i == 0 { (16, 8) } else { (4, 16) };
            let image = hp_vtf::RgbaImage::new(w, h, color.repeat(w as usize * h as usize)).unwrap();
            let opts =
                hp_vtf::EncodeOptions { format: hp_vtf::EncodeFormat::Bgra8888, mipmaps: false, ..Default::default() };
            std::fs::write(mats.join(format!("layer{i}.vtf")), hp_vtf::encode(&image, &opts).unwrap()).unwrap();
        }
        for (name, shader, params) in [
            ("two", "WorldVertexTransition", "$basetexture2 t/layer1"),
            ("four", "Lightmapped_4WayBlend", "$basetexture2 t/layer1 $basetexture3 t/layer2 $basetexture4 t/layer3"),
            ("missing", "WorldVertexTransition", "$basetexture2 t/missing"),
            ("plain", "LightmappedGeneric", "$basetexture2 t/layer1"),
        ] {
            std::fs::write(mats.join(format!("{name}.vmt")), format!("{shader} {{ $basetexture t/layer0 {params} }}"))
                .unwrap();
        }
        let fs = hp_fs::GameFs::empty();
        fs.add_game_dir(&dir.join("g")).unwrap();
        let pixel = |tex: &ThumbPixels, x: usize, y: usize| -> [u8; 4] {
            tex.rgba[(y * tex.width as usize + x) * 4..][..4].try_into().unwrap()
        };
        let opaque = |i: usize| [colors[i][0], colors[i][1], colors[i][2], 255];
        let two = thumb_job(&fs, "t/two", false).unwrap();
        assert_eq!((two.width, two.height, two.size), (16, 8, [16, 8]));
        assert_eq!(pixel(&two, 0, 0), opaque(0));
        assert_eq!(pixel(&two, 15, 7), opaque(1));
        let four = thumb_job(&fs, "t/four", false).unwrap();
        for (i, (x, y)) in [(0, 0), (15, 0), (0, 7), (15, 7)].into_iter().enumerate() {
            assert_eq!(pixel(&four, x, y), opaque(i));
        }
        for name in ["t/missing", "t/plain"] {
            let tex = thumb_job(&fs, name, false).unwrap();
            assert!(tex.rgba.chunks_exact(4).all(|p| p == opaque(0)));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod mat_fuzz_tmp {
    #[test]
    #[ignore = "needs a local Garry's Mod install (hard coded path) and takes minutes: run with --ignored"]
    fn every_material_and_thumb_without_panic() {
        let root = std::path::Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod");
        let fs = hp_fs::GameFs::for_gmod(root, &hp_fs::MountOptions::default()).expect("mount");
        let mats = fs.materials();
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let mut bad = Vec::new();
        for m in mats.iter() {
            let name = m.name.clone();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = super::thumb_job(&fs, &name, false);
                let _ = crate::material::material_job(&fs, &name, false, false);
            }));
            if r.is_err() {
                bad.push(name);
            }
        }
        std::panic::set_hook(prev);
        eprintln!("MATFUZZ {} materials, {} panics: {:?}", mats.len(), bad.len(), &bad[..bad.len().min(30)]);
    }
}
