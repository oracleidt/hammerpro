use std::time::Instant;

use ahash::AHashSet;
use hp_render::{MaterialId, ModelId};

use crate::AssetManager;
use crate::material::{AnaLook, MatAnalysis, MatKind};
use crate::queue::Lane;
use crate::registry::MatSource;
use crate::residency::{self, LOWER_AFTER, MAX_STREAMING, Need, UPDATE_INTERVAL};
use crate::texture::MAX_TEXTURE_DIM;

const PRELOAD_HOLD: std::time::Duration = std::time::Duration::from_secs(3);
pub(crate) const PRELOAD_CAP: u32 = 256;
const FRESH_WAIT_FRAMES: u64 = 30;
const FRESH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const FRESH_STREAMING: usize = 256;
const FRESH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(50);
const MAX_DOWNGRADES: usize = 256;

enum Want {
    Full,
    Placed(Vec<(Need, f32)>),
}

fn transform_factor(a: &MatAnalysis) -> f32 {
    let row = |x: f32, y: f32| (x * x + y * y).sqrt();
    let f = row(a.uv[0], a.uv_cross[0]).max(row(a.uv_cross[1], a.uv[1]));
    if f.is_finite() { f.max(1.0) } else { f32::INFINITY }
}

fn base_role(a: &MatAnalysis, key: &str) -> bool {
    a.water.is_none()
        && (a.texture.as_deref() == Some(key) || (a.four_way.is_none() && a.texture2.as_deref() == Some(key)))
}

pub(crate) fn full_dim(size: [u32; 2]) -> u32 {
    size[0].max(size[1]).clamp(1, MAX_TEXTURE_DIM)
}

impl AssetManager {
    pub(crate) fn escape_models(&mut self, ids: impl Iterator<Item = ModelId>) {
        let frame = self.shared.current_frame();
        for id in ids {
            match self.residency.models.get(&id) {
                Some(meta) => {
                    let mats: Vec<MaterialId> = meta.mats.iter().map(|m| m.0).collect();
                    for m in mats {
                        self.residency.escape(m, frame);
                    }
                }
                None => self.residency.models_escaped = Some(frame),
            }
        }
    }

    pub(crate) fn hold_streaming_for_preload(&mut self) {
        self.residency.preload_until = Some(Instant::now() + PRELOAD_HOLD);
    }

    pub(crate) fn first_cap(&self, key: &str, id: MaterialId, a: &MatAnalysis) -> u32 {
        let opening =
            self.residency.preload_until.is_some_and(|t| Instant::now() < t) || self.residency.fresh.is_some();
        let streaming = hp_app::runtime::render::render_display().texture_streaming;
        let kind = self.mats.records.get((id as usize).wrapping_sub(1)).map(|r| &r.source);
        let placed = matches!(kind, Some(MatSource::Asset { kind: MatKind::World | MatKind::Model, .. }));
        if opening && streaming && placed && !key.contains('|') && base_role(a, key) {
            PRELOAD_CAP
        } else {
            MAX_TEXTURE_DIM
        }
    }

    pub(crate) fn begin_fresh_scene(&mut self) {
        self.residency.preload_until = None;
        self.residency.views.clear();
        self.residency.fresh = Some((Instant::now(), self.shared.current_frame()));
        self.residency.last = None;
    }

    pub(crate) fn fresh_scene_ready(&self) -> bool {
        self.residency.fresh.is_none()
    }

    fn live_model_ids(&self) -> AHashSet<ModelId> {
        let mut live: AHashSet<ModelId> = self.models.render.values().filter_map(|m| m.id).collect();
        live.extend(self.models.variants.values().filter_map(|(m, _)| m.id));
        live.extend(self.particle_draws.live_models());
        live.extend(self.ragdolls.live_models());
        live
    }

    fn texture_want(&self, key: &str, needs: &[Need], escaped: &AHashSet<MaterialId>, models_unknown: bool) -> Want {
        if key.contains('|') {
            return Want::Full;
        }
        let Some(users) = self.mats.tex_users.get(key) else { return Want::Full };
        let mut out = Vec::with_capacity(users.len());
        for &id in users {
            let Some(record) = self.mats.records.get((id as usize).wrapping_sub(1)) else { return Want::Full };
            let MatSource::Asset { name, kind } = &record.source else { return Want::Full };
            if !matches!(kind, MatKind::World | MatKind::Model) || escaped.contains(&id) {
                return Want::Full;
            }
            if *kind == MatKind::Model && models_unknown {
                return Want::Full;
            }
            let AnaLook::Ready(a) = self.mats.ana_look(name) else { return Want::Full };
            if !base_role(a, key) {
                return Want::Full;
            }
            let Some(need) = needs.get(id as usize).copied().filter(|n| n.placed && !n.full) else {
                return Want::Full;
            };
            out.push((need, transform_factor(a)));
        }
        if out.is_empty() { Want::Full } else { Want::Placed(out) }
    }

    pub(crate) fn update_residency(&mut self) {
        let now = Instant::now();
        let fresh = self.residency.fresh;
        let interval = if fresh.is_some() { FRESH_INTERVAL } else { UPDATE_INTERVAL };
        if let Some(last) = self.residency.last
            && now.duration_since(last) < interval
        {
            if let Some(ctx) = self.shared.ctx() {
                ctx.request_repaint_after(interval - now.duration_since(last));
            }
            return;
        }
        self.residency.last = Some(now);
        let frame = self.shared.current_frame();
        let enabled = hp_app::runtime::render::render_display().texture_streaming;
        let views = self.residency.live_views(frame);
        let live = self.live_model_ids();
        self.residency.prune_models(&live);
        let escaped = self.residency.escaped_now(frame);
        let models_unknown = self.scene.placements.unknown_models
            || self.residency.models_escaped.is_some_and(|f| frame.saturating_sub(f) <= residency::ESCAPE_TTL);
        let restore = !enabled || self.play_world_shown() || self.fs.is_none();
        let active = !restore && !views.is_empty();
        let scene_busy = !self.scene.initialized
            || self.scene.prep.is_some()
            || !self.scene.upload_pending.is_empty()
            || !self.scene.realize_later.is_empty()
            || self.scene.instances_streamed;
        let sky = self.scene.env.room.as_ref().filter(|r| r.scale.is_finite() && r.scale >= 1.0).map(|r| {
            let mut objects: AHashSet<u32> = r.solids.iter().copied().collect();
            objects.extend(r.entities.iter().map(|e| crate::entities::entity_object(*e)));
            residency::SkyFrame { origin: r.origin.as_vec3(), inv_scale: 1.0 / r.scale, objects }
        });
        let preloading = self.residency.preload_until.is_some_and(|t| now < t);
        let holding = scene_busy || fresh.is_some() || preloading;
        let mut needs = if active {
            residency::needs(&self.scene.placements, &views, self.mats.records.len(), sky.as_ref())
        } else {
            Vec::new()
        };
        if active && hp_app::runtime::render::render_display().cubemaps {
            let key = (self.scene.placements.revision, self.editor_cubes.revision());
            let count = self.mats.records.len();
            let cached = self.residency.probes.as_ref().filter(|(k, n)| *k == key && n.len() == count + 1);
            let probe = match cached {
                Some((_, n)) => std::sync::Arc::clone(n),
                None => {
                    let views = self.editor_cubes.probe_views();
                    let n = std::sync::Arc::new(residency::probe_needs(&self.scene.placements, &views, count));
                    self.residency.probes = Some((key, std::sync::Arc::clone(&n)));
                    n
                }
            };
            residency::merge_needs(&mut needs, &probe);
        }
        struct Item {
            key: String,
            full: u32,
            cap: u32,
            top: u32,
            bytes: u64,
            want: Option<Want>,
        }
        let mut items: Vec<Item> = Vec::with_capacity(self.mats.textures.map.len());
        for (key, entry) in &self.mats.textures.map {
            let Some((_, size)) = entry.current else { continue };
            if entry.loading || entry.missing {
                continue;
            }
            let full = full_dim(size);
            let cap = if entry.stream.cap == 0 { full } else { entry.stream.cap.min(full) };
            let reduced = cap < full;
            let eligible = !key.contains('|') && (entry.stream.levels >= 2 || reduced);
            let want = if !eligible {
                None
            } else if restore || !active {
                Some(Want::Full)
            } else {
                Some(self.texture_want(key, &needs, &escaped, models_unknown))
            };
            items.push(Item { key: key.clone(), full, cap, top: entry.stream.top, bytes: entry.stream.bytes, want });
        }
        let want_at = |item: &Item, a: f32| -> u32 {
            match &item.want {
                None => item.cap,
                Some(Want::Full) => item.full,
                Some(Want::Placed(list)) => {
                    list.iter().map(|(n, f)| n.cap(item.full, *f, a)).max().unwrap_or(item.full).min(item.full)
                }
            }
        };
        let aggression = if active {
            residency::aggression_for(self.residency.budget, |a| {
                items
                    .iter()
                    .map(|i| match i.want {
                        None => i.bytes,
                        Some(_) => residency::texture_bytes(i.bytes, i.top, want_at(i, a)),
                    })
                    .sum()
            })
        } else {
            1.0
        };
        let mut report = hp_app::runtime::TextureStreaming {
            enabled,
            active,
            budget: self.residency.budget,
            aggression: aggression as u32,
            ..Default::default()
        };
        let mut wake: Option<std::time::Duration> = None;
        let mut upgrades: Vec<(u32, String, u32)> = Vec::new();
        let mut downgrades: Vec<(String, u32)> = Vec::new();
        for item in &items {
            report.bytes += item.bytes;
            if item.want.is_none() {
                report.full_bytes += item.bytes;
                continue;
            }
            report.full_bytes += residency::texture_bytes(item.bytes, item.top, item.full);
            report.textures += 1;
            if item.cap < item.full {
                report.reduced += 1;
            }
            let want = want_at(item, aggression);
            let Some(entry) = self.mats.textures.map.get_mut(&item.key) else { continue };
            if entry.stream.pending {
                continue;
            }
            if want > item.cap {
                entry.stream.lower_since = None;
                upgrades.push((want / item.cap.max(1), item.key.clone(), want));
            } else if want < item.cap && want < item.top.max(1) && !holding && active {
                let since = *entry.stream.lower_since.get_or_insert(now);
                let waited = now.duration_since(since);
                if waited >= LOWER_AFTER {
                    downgrades.push((item.key.clone(), want));
                } else {
                    wake =
                        Some(wake.map_or(LOWER_AFTER - waited, |w: std::time::Duration| w.min(LOWER_AFTER - waited)));
                }
            } else {
                entry.stream.lower_since = None;
            }
        }
        if preloading {
            upgrades.clear();
            downgrades.clear();
        }
        let mut pending = self.mats.textures.map.values().filter(|e| e.stream.pending).count();
        let limit = if fresh.is_some() { FRESH_STREAMING } else { MAX_STREAMING };
        let waiting_upgrades = upgrades.len();
        upgrades.sort_unstable_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        for (_, key, cap) in upgrades {
            if pending >= limit {
                break;
            }
            if self.queue_stream(&key, cap, Lane::High) {
                pending += 1;
            }
        }
        if let Some((since, first_frame)) = fresh {
            let loading = self.mats.textures.map.values().any(|e| e.loading);
            let settled = !scene_busy && !loading && waiting_upgrades == 0 && pending == 0;
            let blind = views.is_empty() && frame.saturating_sub(first_frame) > FRESH_WAIT_FRAMES;
            let ready = (active || restore) && settled;
            if ready || blind || now.duration_since(since) > FRESH_TIMEOUT {
                self.residency.fresh = None;
            }
        }
        downgrades.sort_unstable();
        for (key, cap) in downgrades {
            if pending >= MAX_DOWNGRADES {
                break;
            }
            if self.queue_stream(&key, cap, Lane::Low) {
                pending += 1;
            }
        }
        if let (Some(after), Some(ctx)) = (wake, self.shared.ctx()) {
            ctx.request_repaint_after(after + std::time::Duration::from_millis(10));
        }
        report.pending = pending;
        self.residency.aggression = aggression;
        self.residency.report = report;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::AnaEntry;
    use crate::residency::{Density, Placement};
    use crate::texture::TexEntry;
    use std::sync::Arc;

    fn manager_with(materials: &[(&str, MatKind, &str)]) -> (AssetManager, Vec<MaterialId>) {
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        let mut ids = Vec::new();
        for (name, kind, vmt) in materials {
            let material = hp_vmt::parse(vmt).unwrap();
            let analysis = Arc::new(crate::material::analyze(name, &material, &|_| 0));
            assets
                .mats
                .analyses
                .insert((*name).to_owned(), AnaEntry { value: Some(Some(analysis)), ..Default::default() });
            ids.push(assets.asset_material(name, *kind));
        }
        (assets, ids)
    }

    fn uploaded(assets: &mut AssetManager, key: &str, dim: u32) {
        let upload = hp_render::TextureUpload {
            width: dim,
            height: dim,
            format: hp_render::TexFormat::Bc1Srgb,
            levels: (0..dim.ilog2()).map(|l| vec![0u8; ((dim >> l).max(4) as usize).pow(2) / 2]).collect(),
        };
        let entry = TexEntry {
            current: Some((hp_render::TexId::WHITE, [dim, dim])),
            stream: crate::texture::StreamState::uploaded(MAX_TEXTURE_DIM, &upload),
            ..Default::default()
        };
        assets.mats.textures.map.insert(key.to_owned(), entry);
    }

    fn eye(assets: &mut AssetManager, eye: [f32; 3]) {
        let params = hp_render::ViewParams {
            size: [1600, 900],
            kind: hp_render::ViewKind::Perspective {
                eye,
                forward: [1.0, 0.0, 0.0],
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 60.0,
                near: 1.0,
                far: 60000.0,
            },
            mode: hp_render::RenderMode::Textured,
            grid: None,
            background: [0.0; 3],
            selection_color: [0.0; 4],
            show_sprites: true,
            show_models: true,
            msaa: false,
            game_pixel_grid: false,
        };
        hp_app::AssetsApi::note_view(assets, 0, &params);
    }

    fn refresh(assets: &mut AssetManager) {
        assets.residency.last = None;
        assets.update_residency();
    }

    fn far_wall(material: MaterialId) -> Placement {
        Placement {
            min: [30_000.0, 0.0, 0.0],
            max: [30_128.0, 128.0, 128.0],
            mats: vec![(material, Density::Scale(0.25))],
        }
    }

    #[test]
    fn far_textures_wait_then_shrink_and_shared_or_untracked_ones_stay_full() {
        let (mut assets, ids) = manager_with(&[
            ("t/far", MatKind::World, r#"LightmappedGeneric { $basetexture t/far }"#),
            ("t/shared", MatKind::World, r#"LightmappedGeneric { $basetexture t/shared }"#),
            ("t/shared_sprite", MatKind::Sprite, r#"Sprite { $basetexture t/shared }"#),
            ("t/detailed", MatKind::World, r#"LightmappedGeneric { $basetexture t/base $detail t/far }"#),
        ]);
        for key in ["t/far", "t/shared", "t/base"] {
            uploaded(&mut assets, key, 1024);
        }
        assets.scene.placements.set_object(1, far_wall(ids[0]));
        assets.scene.placements.set_object(2, far_wall(ids[1]));
        eye(&mut assets, [0.0; 3]);
        assets.shared.env.write().replace(crate::queue::FsEnv { fs: hp_fs::GameFs::empty(), epoch: 0 });
        assets.fs = Some(hp_fs::GameFs::empty());
        assets.scene.initialized = true;
        refresh(&mut assets);
        assert!(assets.residency.report.active);
        assert_eq!(assets.shared.queued(), 0, "nothing shrinks before the delay");
        let far = assets.mats.textures.map.get_mut("t/far").unwrap();
        far.stream.lower_since = Some(Instant::now() - LOWER_AFTER);
        refresh(&mut assets);
        assert!(!assets.mats.textures.map["t/far"].stream.pending, "used as a detail texture too: kept full");
        assets.mats.tex_users.get_mut("t/far").unwrap().retain(|id| *id == ids[0]);
        assets.mats.textures.map.get_mut("t/far").unwrap().stream.lower_since = Some(Instant::now() - LOWER_AFTER);
        refresh(&mut assets);
        assert!(assets.mats.textures.map["t/far"].stream.pending);
        assert!(!assets.mats.textures.map["t/shared"].stream.pending, "also used as a sprite");
        assert!(assets.mats.textures.map["t/shared"].stream.lower_since.is_none());
        assets.mats.textures.map.get_mut("t/far").unwrap().stream.pending = false;
        assets.scene.upload_pending.insert(9);
        assets.mats.textures.map.get_mut("t/far").unwrap().stream.lower_since = Some(Instant::now() - LOWER_AFTER);
        refresh(&mut assets);
        assert!(!assets.mats.textures.map["t/far"].stream.pending, "nothing shrinks while the map is still uploading");
    }

    #[test]
    fn reduced_textures_come_back_when_the_camera_closes_in_or_streaming_is_off() {
        let (mut assets, ids) =
            manager_with(&[("t/wall", MatKind::World, r#"LightmappedGeneric { $basetexture t/wall }"#)]);
        uploaded(&mut assets, "t/wall", 1024);
        assets.mats.textures.map.get_mut("t/wall").unwrap().stream.cap = 128;
        assets.scene.placements.set_object(1, far_wall(ids[0]));
        assets.shared.env.write().replace(crate::queue::FsEnv { fs: hp_fs::GameFs::empty(), epoch: 0 });
        assets.fs = Some(hp_fs::GameFs::empty());
        eye(&mut assets, [30_000.0, 64.0, 64.0]);
        assets.hold_streaming_for_preload();
        refresh(&mut assets);
        assert!(!assets.mats.textures.map["t/wall"].stream.pending, "nothing streams while a map preloads");
        assets.residency.preload_until = None;
        refresh(&mut assets);
        assert!(assets.mats.textures.map["t/wall"].stream.pending, "the camera stands at the wall");
        assert_eq!(assets.residency.report.reduced, 1);
        assets.mats.textures.map.get_mut("t/wall").unwrap().stream.pending = false;
        let mut display = hp_app::runtime::render::render_display();
        display.texture_streaming = false;
        hp_app::runtime::render::publish_render_display(display);
        eye(&mut assets, [0.0; 3]);
        refresh(&mut assets);
        assert!(!assets.residency.report.active);
        assert!(assets.mats.textures.map["t/wall"].stream.pending, "turning streaming off restores full size");
        display.texture_streaming = true;
        hp_app::runtime::render::publish_render_display(display);
    }

    #[test]
    fn maps_open_small_then_show_once_near_textures_are_full() {
        let (mut assets, ids) =
            manager_with(&[("t/wall", MatKind::World, r#"LightmappedGeneric { $basetexture t/wall $detail t/fine }"#)]);
        let analysis = match assets.mats.ana_look("t/wall") {
            AnaLook::Ready(a) => a.clone(),
            _ => panic!("analysis"),
        };
        assert_eq!(assets.first_cap("t/wall", ids[0], &analysis), MAX_TEXTURE_DIM, "not opening a map");
        assets.hold_streaming_for_preload();
        assert_eq!(assets.first_cap("t/wall", ids[0], &analysis), PRELOAD_CAP);
        assert_eq!(assets.first_cap("t/fine", ids[0], &analysis), MAX_TEXTURE_DIM, "detail textures stay full");
        assets.residency.preload_until = None;
        uploaded(&mut assets, "t/wall", 1024);
        if let Some(fine) = assets.mats.textures.map.get_mut("t/fine") {
            fine.loading = false;
            fine.missing = true;
        }
        assets.mats.textures.map.get_mut("t/wall").unwrap().stream.cap = PRELOAD_CAP;
        assets.shared.env.write().replace(crate::queue::FsEnv { fs: hp_fs::GameFs::empty(), epoch: 0 });
        assets.fs = Some(hp_fs::GameFs::empty());
        assets.scene.initialized = true;
        assets.begin_fresh_scene();
        assert!(!assets.fresh_scene_ready());
        let near = Placement { min: [0.0; 3], max: [128.0; 3], mats: vec![(ids[0], Density::Scale(0.25))] };
        assets.scene.placements.set_object(1, near);
        refresh(&mut assets);
        assert!(!assets.fresh_scene_ready(), "no view yet");
        eye(&mut assets, [64.0, 64.0, 64.0]);
        refresh(&mut assets);
        assert!(assets.mats.textures.map["t/wall"].stream.pending, "the near texture is loaded at full size");
        assert!(!assets.fresh_scene_ready());
        let wall = assets.mats.textures.map.get_mut("t/wall").unwrap();
        wall.stream.pending = false;
        wall.stream.cap = 1024;
        assets.mats.textures.map.insert("t/late".into(), TexEntry { loading: true, ..Default::default() });
        refresh(&mut assets);
        assert!(!assets.fresh_scene_ready(), "a texture still loading keeps the loading screen");
        assets.mats.textures.map.remove("t/late");
        refresh(&mut assets);
        assert!(assets.fresh_scene_ready(), "the map shows once its near textures are at full size");
        assets.begin_fresh_scene();
        let first = assets.shared.current_frame();
        refresh(&mut assets);
        assert!(!assets.fresh_scene_ready());
        assets.shared.frame.store(first + FRESH_WAIT_FRAMES + 1, std::sync::atomic::Ordering::Relaxed);
        refresh(&mut assets);
        assert!(assets.fresh_scene_ready(), "no textured view: nothing to wait for");
    }

    #[test]
    fn textures_requested_while_a_map_preloads_start_small() {
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        assets.hold_streaming_for_preload();
        let id = assets.asset_material("t/wall", MatKind::World);
        let material = hp_vmt::parse(r#"LightmappedGeneric { $basetexture t/wall }"#).unwrap();
        let analysis = Arc::new(crate::material::analyze("t/wall", &material, &|_| 0));
        let info = Arc::new(crate::material::make_info("t/wall", None, None, (0, 0), 0));
        assets.on_material_for_tests("t/wall", crate::material::MaterialData { analysis: Some(analysis), info });
        assert!(id > 0);
        assert_eq!(assets.mats.textures.map["t/wall"].stream.cap, PRELOAD_CAP);
    }

    #[test]
    fn special_textures_count_at_their_real_size_in_the_report() {
        let (mut assets, ids) =
            manager_with(&[("t/wall", MatKind::World, r#"LightmappedGeneric { $basetexture t/wall }"#)]);
        uploaded(&mut assets, "t/wall", 1024);
        let cube = hp_render::TextureUpload {
            width: 1024,
            height: 6144,
            format: hp_render::TexFormat::Bc1Srgb,
            levels: vec![vec![0u8; 1024 * 6144 / 2]],
        };
        let entry = TexEntry {
            current: Some((hp_render::TexId::WHITE, [1024, 6144])),
            stream: crate::texture::StreamState::uploaded(MAX_TEXTURE_DIM, &cube),
            ..Default::default()
        };
        assets.mats.textures.map.insert("t/sky|cube".to_owned(), entry);
        assets.scene.placements.set_object(1, far_wall(ids[0]));
        assets.shared.env.write().replace(crate::queue::FsEnv { fs: hp_fs::GameFs::empty(), epoch: 0 });
        assets.fs = Some(hp_fs::GameFs::empty());
        assets.scene.initialized = true;
        eye(&mut assets, [0.0; 3]);
        refresh(&mut assets);
        let report = assets.residency.report;
        assert_eq!(report.textures, 1, "the cubemap is not streamed");
        assert!(report.full_bytes >= report.bytes, "{} < {}", report.full_bytes, report.bytes);
        assert_eq!(report.full_bytes, report.bytes, "nothing is reduced yet");
    }

    #[test]
    fn updates_are_spaced_and_opening_maps_are_checked_more_often() {
        let (mut assets, ids) =
            manager_with(&[("t/wall", MatKind::World, r#"LightmappedGeneric { $basetexture t/wall }"#)]);
        uploaded(&mut assets, "t/wall", 1024);
        assets.scene.placements.set_object(1, far_wall(ids[0]));
        assets.shared.env.write().replace(crate::queue::FsEnv { fs: hp_fs::GameFs::empty(), epoch: 0 });
        assets.fs = Some(hp_fs::GameFs::empty());
        assets.scene.initialized = true;
        eye(&mut assets, [0.0; 3]);
        refresh(&mut assets);
        assert_eq!(assets.residency.report.textures, 1);
        assets.mats.textures.map.remove("t/wall");
        assets.update_residency();
        assert_eq!(assets.residency.report.textures, 1, "too soon after the last update: skipped");
        assets.residency.last = Some(Instant::now() - UPDATE_INTERVAL);
        assets.update_residency();
        assert_eq!(assets.residency.report.textures, 0);
        assets.mats.textures.map.insert("t/late".into(), TexEntry { loading: true, ..Default::default() });
        assets.begin_fresh_scene();
        assets.update_residency();
        assert!(!assets.fresh_scene_ready());
        let checked = Instant::now() - FRESH_INTERVAL;
        assets.residency.last = Some(checked);
        assets.update_residency();
        assert!(assets.residency.last.is_some_and(|t| t > checked), "a map that opens is checked every 50 ms");
    }

    #[test]
    fn transforms_and_roles() {
        let m = hp_vmt::parse(r#"LightmappedGeneric { $basetexture a $basetexturetransform "center .5 .5 scale 4 4 rotate 0 translate 0 0" }"#)
            .unwrap();
        let a = crate::material::analyze("x", &m, &|_| 0);
        assert!((transform_factor(&a) - 4.0).abs() < 1e-4);
        assert!(base_role(&a, "a"));
        assert!(!base_role(&a, "b"));
        assert_eq!(full_dim([4096, 512]), MAX_TEXTURE_DIM);
        assert_eq!(full_dim([256, 512]), 512);
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    use crate::texture::{TexEntry, TexTarget};

    fn wall_vtf(size: u32) -> Vec<u8> {
        let img = hp_vtf::RgbaImage::new(size, size, [90u8, 120, 200, 255].repeat((size * size) as usize)).unwrap();
        let opts = hp_vtf::EncodeOptions { format: hp_vtf::EncodeFormat::Dxt1, mipmaps: true, ..Default::default() };
        hp_vtf::encode(&img, &opts).unwrap()
    }

    #[test]
    fn capped_loads_read_smaller_mips_and_keep_the_native_size() {
        let dir = std::env::temp_dir().join(format!("hp-stream-cap-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("materials/t")).unwrap();
        std::fs::write(dir.join("materials/t/wall.vtf"), wall_vtf(512)).unwrap();
        let fs = hp_fs::GameFs::empty();
        fs.add_game_dir(&dir).unwrap();
        let full = crate::texture::prepare_texture(&fs, "t/wall", true, MAX_TEXTURE_DIM).unwrap();
        assert_eq!((full.upload.width, full.size), (512, [512, 512]));
        let small = crate::texture::prepare_texture(&fs, "t/wall", true, 128).unwrap();
        assert_eq!((small.upload.width, small.upload.height, small.size), (128, 128, [512, 512]));
        assert_eq!(small.upload.levels.len(), full.upload.levels.len() - 2);
        assert_eq!(small.upload.levels[0], full.upload.levels[2], "the same mip data, without a decode");
        let sprite = crate::texture::prepare_texture(&fs, "t/wall|linear", true, 128).unwrap();
        assert_eq!(sprite.upload.width, 512, "special keys ignore the cap");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stream_results_replace_the_texture_and_failures_keep_it() {
        let Some((device, queue)) = crate::test_gpu::gpu() else { return };
        let mut renderer = hp_render::Renderer::new(&device, &queue);
        let mut assets = AssetManager::new(&hp_app::settings::Settings::default());
        let level = |dim: u32| vec![0u8; ((dim / 4).max(1) * (dim / 4).max(1) * 8) as usize];
        let prepared = |dim: u32| crate::texture::PreparedTexture {
            upload: hp_render::TextureUpload {
                width: dim,
                height: dim,
                format: hp_render::TexFormat::Bc1Srgb,
                levels: vec![level(dim), level(dim / 2)],
            },
            size: [1024, 1024],
        };
        let first = renderer.create_texture_exact(&prepared(1024).upload);
        assets.mats.textures.map.insert(
            "t/wall".into(),
            TexEntry {
                current: Some((first, [1024, 1024])),
                stream: crate::texture::StreamState::uploaded(MAX_TEXTURE_DIM, &prepared(1024).upload),
                ..Default::default()
            },
        );
        let upload = |assets: &AssetManager, data: Option<crate::texture::PreparedTexture>, cap: u32| crate::Upload {
            name: "t/wall".into(),
            target: TexTarget::Main,
            epoch: assets.mount_gen,
            cap,
            stream: true,
            data,
            _held: assets.shared.inflight.hold(0),
        };
        let up = upload(&assets, Some(prepared(256)), 256);
        assets.finish_texture_main(&mut renderer, up);
        assert_eq!(assets.mats.textures.map["t/wall"].current.unwrap().0, first, "not requested: ignored");
        assets.mats.textures.map.get_mut("t/wall").unwrap().stream.pending = true;
        let up = upload(&assets, Some(prepared(256)), 256);
        assets.finish_texture_main(&mut renderer, up);
        let entry = assets.mats.textures.map["t/wall"];
        assert_ne!(entry.current.unwrap().0, first);
        assert_eq!(entry.current.unwrap().1, [1024, 1024], "the shader keeps the native size");
        assert_eq!((entry.stream.cap, entry.stream.top, entry.stream.pending), (256, 256, false));
        assert!(renderer.texture_view(first).is_none(), "the full size texture is freed");
        let kept = entry.current.unwrap().0;
        assets.mats.textures.map.get_mut("t/wall").unwrap().stream.pending = true;
        let up = upload(&assets, None, MAX_TEXTURE_DIM);
        assets.finish_texture_main(&mut renderer, up);
        let entry = assets.mats.textures.map["t/wall"];
        assert_eq!(entry.current.unwrap().0, kept, "a failed reload keeps what is shown");
        assert!(!entry.stream.pending && !entry.missing);
    }
}
