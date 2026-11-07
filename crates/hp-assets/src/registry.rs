use std::borrow::Cow;
use std::sync::Arc;
use std::time::Duration;

use ahash::{AHashMap, AHashSet};
use hp_app::state::{AssetStatus, Gfx, MaterialInfo, ModelState};
use hp_app::{t, tf};
use hp_render::{MaterialDesc, MaterialId, Renderer, TexId};

use crate::entities::{class_of, local_bounds, model_path, model_scale};
use crate::jobs::{AssetLists, Job, JobResult, MountOutcome};
use crate::material::{AnaLook, MatAnalysis, MatKind, asset_desc, flat_desc, guess_tool, texture_keys};
use crate::models::{ModelEntry, entity_matrix, transformed_bounds};
use crate::mount::{FgdRequest, MountRequest, run_fgd, run_mount};
use crate::queue::{BoundsState, FsEnv, Lane, Shared};
use crate::texture::{TexRegistry, TexTarget};
use crate::util::{Budget, is_normalized_material, material_key, model_key, model_key_cow};
use crate::{AssetManager, Upload};

const UPLOAD_BUDGET: Duration = Duration::from_millis(4);
const UPLOAD_BUDGET_BULK: Duration = Duration::from_millis(14);
const BULK_UPLOADS: usize = 24;
const FGD_NOTES_SHOWN: usize = 20;
const MOUNT_SOURCES_SHOWN: usize = 40;

pub(crate) enum MatSource {
    Asset { name: String, kind: MatKind },
    Flat([u8; 4]),
}

pub(crate) struct MatRecord {
    pub source: MatSource,
    pub dirty: bool,
    pub epoch: u64,
    pub cold: bool,
}

#[derive(Default)]
pub(crate) struct AnaEntry {
    pub value: Option<Option<Arc<MatAnalysis>>>,
    pub loading: bool,
    pub queued: bool,
}

#[derive(Default)]
pub(crate) struct Materials {
    pub hdr: bool,
    pub mode_epoch: u64,
    pub records: Vec<MatRecord>,
    pub by_name: [AHashMap<String, MaterialId>; 5],
    pub raw_world: AHashMap<String, MaterialId>,
    pub flat: AHashMap<[u8; 4], MaterialId>,
    pub dirty: Vec<MaterialId>,
    pub analyses: AHashMap<String, AnaEntry>,
    pub infos: AHashMap<String, Arc<MaterialInfo>>,
    pub users: AHashMap<String, Vec<MaterialId>>,
    pub tex_users: AHashMap<String, Vec<MaterialId>>,
    pub textures: TexRegistry,
    pub hide_tools: bool,
    pub force_tex: AHashSet<String>,
    pub scene_epoch: u64,
}

fn kind_index(kind: MatKind) -> usize {
    match kind {
        MatKind::World => 0,
        MatKind::Model => 1,
        MatKind::Sprite => 2,
        MatKind::SpriteAdditive => 3,
        MatKind::SpriteRefract => 4,
    }
}

impl Materials {
    fn record_mut(&mut self, id: MaterialId) -> Option<&mut MatRecord> {
        self.records.get_mut((id as usize).wrapping_sub(1))
    }

    pub(crate) fn mark_dirty(&mut self, id: MaterialId) {
        if let Some(r) = self.record_mut(id)
            && !r.dirty
        {
            r.dirty = true;
            self.dirty.push(id);
        }
    }

    fn mark_texture_users(&mut self, texture: &str) {
        let ids = self.tex_users.get(texture).cloned().unwrap_or_default();
        for id in ids {
            self.mark_dirty(id);
        }
    }

    pub(crate) fn ana_look(&self, name: &str) -> AnaLook<'_> {
        match self.analyses.get(name).and_then(|e| e.value.as_ref()) {
            Some(Some(a)) => AnaLook::Ready(a),
            Some(None) => AnaLook::Missing,
            None => AnaLook::Loading,
        }
    }

    pub(crate) fn desc(&self, id: MaterialId) -> Option<MaterialDesc> {
        let record = self.records.get((id as usize).wrapping_sub(1))?;
        Some(match &record.source {
            MatSource::Flat(rgba) => flat_desc(*rgba),
            MatSource::Asset { name, kind } => {
                asset_desc(*kind, name, self.ana_look(name), &|t| self.textures.look(t), !self.hide_tools)
            }
        })
    }

    pub(crate) fn apply_dirty(&mut self, renderer: &mut Renderer) {
        if self.dirty.is_empty() {
            return;
        }
        let dirty = std::mem::take(&mut self.dirty);
        for id in dirty {
            if let Some(desc) = self.desc(id) {
                renderer.set_material(id, &desc);
            }
            if let Some(r) = self.record_mut(id) {
                r.dirty = false;
            }
        }
    }

    pub(crate) fn set_hide_tools(&mut self, hide: bool) {
        if self.hide_tools == hide {
            return;
        }
        self.hide_tools = hide;
        let tools: Vec<MaterialId> = self
            .records
            .iter()
            .enumerate()
            .filter(|(_, r)| match &r.source {
                MatSource::Asset { name, .. } => {
                    guess_tool(name).is_some() || matches!(self.ana_look(name), AnaLook::Ready(a) if a.tool.is_some())
                }
                MatSource::Flat(_) => false,
            })
            .map(|(i, _)| i as MaterialId + 1)
            .collect();
        for id in tools {
            self.mark_dirty(id);
        }
    }
}

pub(crate) fn request_model_shared(shared: &Shared, key: &str) {
    if shared.bounds.read().contains_key(key) {
        return;
    }
    let Some(env) = shared.env() else { return };
    {
        let mut bounds = shared.bounds.write();
        if bounds.contains_key(key) {
            return;
        }
        bounds.insert(key.to_owned(), BoundsState::Pending);
    }
    shared.push(Lane::High, Job::Model { path: key.to_owned(), env });
}

pub(crate) fn request_model_again(shared: &Shared, key: &str) {
    {
        let mut bounds = shared.bounds.write();
        if bounds.get(key).is_some_and(|b| !matches!(b, BoundsState::Pending)) {
            bounds.remove(key);
        }
    }
    request_model_shared(shared, key);
}

pub(crate) fn spawn_task(
    name: &str,
    shared: &Arc<Shared>,
    f: impl FnOnce() -> JobResult + Send + 'static,
    on_panic: impl FnOnce() -> JobResult + Send + 'static,
) -> bool {
    let shared = Arc::clone(shared);
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hp_editor::background::run(f)))
                .unwrap_or_else(|_| on_panic());
            shared.send(result);
        })
        .map_err(|e| log::error!("cannot start thread {name}: {e}"))
        .is_ok()
}

impl AssetManager {
    pub(crate) fn world_material(&mut self, raw: &str) -> MaterialId {
        if let Some(&id) = self.mats.raw_world.get(raw) {
            self.touch_material(id);
            return id;
        }
        let key = material_key(raw);
        let id = self.asset_material(&key, MatKind::World);
        self.mats.raw_world.insert(raw.to_owned(), id);
        id
    }

    pub(crate) fn touch_material(&mut self, id: MaterialId) {
        let epoch = self.mats.scene_epoch;
        let Some(r) = self.mats.record_mut(id) else { return };
        r.epoch = epoch;
        if !r.cold {
            return;
        }
        r.cold = false;
        let name = match &r.source {
            MatSource::Asset { name, .. } => name.clone(),
            MatSource::Flat(_) => return,
        };
        self.mats.mark_dirty(id);
        if let AnaLook::Ready(a) = self.mats.ana_look(&name) {
            let a = a.clone();
            self.want_textures(id, &a);
        }
    }

    pub(crate) fn collect_garbage(&mut self, renderer: &mut Renderer) -> (usize, usize) {
        let epoch = self.gc_epoch();
        let in_use = |mats: &Materials, id: &MaterialId| {
            mats.records.get((*id as usize).wrapping_sub(1)).is_some_and(|r| r.epoch >= epoch)
        };
        let unused: Vec<String> = self
            .mats
            .tex_users
            .iter()
            .filter(|(_, users)| !users.iter().any(|id| in_use(&self.mats, id)))
            .map(|(name, _)| name.clone())
            .collect();
        let mut freed = 0usize;
        for name in &unused {
            if let Some(entry) = self.mats.textures.map.remove(name)
                && let Some((tex, _)) = entry.current
            {
                renderer.destroy_texture(tex);
                freed += 1;
            }
            self.mats.tex_users.remove(name);
        }
        for r in &mut self.mats.records {
            if r.epoch < epoch && matches!(r.source, MatSource::Asset { .. }) {
                r.cold = true;
            }
        }
        let before = self.models.render.len();
        self.models.render.retain(|_, m| {
            if m.epoch >= epoch {
                return true;
            }
            if let Some(id) = m.id {
                renderer.destroy_model(id);
            }
            false
        });
        let models = before - self.models.render.len();
        if freed > 0 || models > 0 {
            log::debug!("released {freed} textures and {models} models not used by the map");
        }
        (freed, models)
    }

    pub(crate) fn asset_material(&mut self, name: &str, kind: MatKind) -> MaterialId {
        if let Some(&id) = self.mats.by_name[kind_index(kind)].get(name) {
            self.touch_material(id);
            return id;
        }
        let id = MaterialId::try_from(self.mats.records.len() + 1).unwrap_or(MaterialId::MAX);
        let epoch = self.mats.scene_epoch;
        self.mats.records.push(MatRecord {
            source: MatSource::Asset { name: name.to_owned(), kind },
            dirty: false,
            epoch,
            cold: false,
        });
        self.mats.by_name[kind_index(kind)].insert(name.to_owned(), id);
        self.mats.users.entry(name.to_owned()).or_default().push(id);
        self.mats.mark_dirty(id);
        self.request_analysis(name, Lane::High);
        if let AnaLook::Ready(a) = self.mats.ana_look(name) {
            let a = a.clone();
            self.want_textures(id, &a);
        }
        id
    }

    pub(crate) fn flat_material(&mut self, rgba: [u8; 4]) -> MaterialId {
        if let Some(&id) = self.mats.flat.get(&rgba) {
            self.touch_material(id);
            return id;
        }
        let id = MaterialId::try_from(self.mats.records.len() + 1).unwrap_or(MaterialId::MAX);
        let epoch = self.mats.scene_epoch;
        self.mats.records.push(MatRecord { source: MatSource::Flat(rgba), dirty: false, epoch, cold: false });
        self.mats.flat.insert(rgba, id);
        self.mats.mark_dirty(id);
        id
    }

    pub(crate) fn request_analysis(&mut self, name: &str, lane: Lane) {
        let entry = self
            .mats
            .analyses
            .entry(name.to_owned())
            .or_insert_with(|| AnaEntry { loading: true, ..AnaEntry::default() });
        if entry.loading && !entry.queued {
            self.queue_analysis(name, lane);
        }
    }

    fn queue_analysis(&mut self, name: &str, lane: Lane) {
        let Some(env) = self.shared.env() else { return };
        let fresh = self.fresh.contains(name);
        if let Some(e) = self.mats.analyses.get_mut(name) {
            e.loading = true;
            e.queued = true;
        }
        self.shared.push(
            lane,
            Job::Material { name: name.to_owned(), fresh, hdr: self.mats.hdr, mode_epoch: self.mats.mode_epoch, env },
        );
    }

    fn material_kind(&self, id: MaterialId) -> MatKind {
        match self.mats.records.get((id as usize).wrapping_sub(1)).map(|r| &r.source) {
            Some(MatSource::Asset { kind, .. }) => *kind,
            _ => MatKind::World,
        }
    }

    fn want_textures(&mut self, id: MaterialId, a: &MatAnalysis) {
        for key in texture_keys(self.material_kind(id), a) {
            let users = self.mats.tex_users.entry(key.clone()).or_default();
            if !users.contains(&id) {
                users.push(id);
            }
            if self.mats.textures.request(&key) {
                self.queue_texture(&key, TexTarget::Main);
            }
        }
    }

    pub(crate) fn refresh_bump_materials(&mut self, renderer: &mut Renderer, on: bool) {
        let mut users: Vec<(MaterialId, Arc<MatAnalysis>, bool)> = Vec::new();
        for (i, r) in self.mats.records.iter().enumerate() {
            let MatSource::Asset { name, .. } = &r.source else { continue };
            if let Some(Some(Some(a))) = self.mats.analyses.get(name).map(|e| e.value.as_ref())
                && a.bump.is_some()
            {
                users.push((i as MaterialId + 1, Arc::clone(a), r.cold));
            }
        }
        for (id, a, cold) in users {
            if (on || a.envmap.is_some()) && !cold {
                self.want_textures(id, &a);
            }
            self.mats.mark_dirty(id);
        }
        if !on {
            let normal: Vec<String> =
                self.mats.textures.map.keys().filter(|k| crate::bump::parse_normal_key(k).is_some()).cloned().collect();
            for key in normal {
                if let Some(e) = self.mats.textures.map.remove(&key)
                    && let Some((id, _)) = e.current
                {
                    renderer.destroy_texture(id);
                }
                self.mats.tex_users.remove(&key);
            }
        }
    }

    pub(crate) fn refresh_envmap_materials(&mut self, renderer: &mut Renderer, on: bool) {
        let mut users: Vec<(MaterialId, Arc<MatAnalysis>, bool)> = Vec::new();
        for (i, r) in self.mats.records.iter().enumerate() {
            let MatSource::Asset { name, .. } = &r.source else { continue };
            if let Some(Some(Some(a))) = self.mats.analyses.get(name).map(|e| e.value.as_ref())
                && a.envmap.is_some()
            {
                users.push((i as MaterialId + 1, Arc::clone(a), r.cold));
            }
        }
        for (id, a, cold) in users {
            if on && !cold {
                self.want_textures(id, &a);
            }
            self.mats.mark_dirty(id);
        }
        if !on {
            let keys: Vec<String> = self
                .mats
                .textures
                .map
                .keys()
                .filter(|k| crate::envmap::parse_mask_key(k).is_some() || k.ends_with(crate::envmap::CUBE_SUFFIX))
                .cloned()
                .collect();
            for key in keys {
                if let Some(e) = self.mats.textures.map.remove(&key)
                    && let Some((id, _)) = e.current
                {
                    renderer.destroy_texture(id);
                }
                self.mats.tex_users.remove(&key);
            }
        }
    }

    pub(crate) fn queue_texture(&mut self, name: &str, target: TexTarget) {
        let Some(env) = self.shared.env() else { return };
        let reg = match target {
            TexTarget::Main => Some(&mut self.mats.textures),
            TexTarget::Preview => self.preview.as_mut().map(|p| &mut p.textures),
        };
        let Some(entry) = reg.and_then(|r| r.map.get_mut(name)) else { return };
        entry.loading = true;
        entry.queued = true;
        let lane = if target == TexTarget::Preview { Lane::Thumb } else { Lane::High };
        self.shared.push(lane, Job::Texture { name: name.to_owned(), target, env });
    }

    pub(crate) fn material_info_impl(&mut self, name: &str) -> Option<Arc<MaterialInfo>> {
        let key: Cow<'_, str> =
            if is_normalized_material(name) { Cow::Borrowed(name) } else { Cow::Owned(material_key(name)) };
        if let Some(info) = self.mats.infos.get(key.as_ref()) {
            return Some(Arc::clone(info));
        }
        if key.is_empty() || (self.fs.is_none() && self.status != AssetStatus::Mounting) {
            let info = crate::material::make_info(&key, None, None, (0, 0), 0);
            return Some(Arc::new(info));
        }
        let entry = self.mats.analyses.entry(key.to_string()).or_default();
        if !entry.loading {
            entry.loading = true;
            entry.queued = false;
        }
        if !entry.queued {
            self.queue_analysis(&key, Lane::Low);
        }
        None
    }

    pub(crate) fn invalidate_material_impl(&mut self, name: &str) {
        let key = material_key(name);
        if key.is_empty() {
            return;
        }
        self.fresh.insert(key.clone());
        self.mats.infos.remove(&key);
        self.thumbs.remove(&key);
        let in_preview = self.preview.as_ref().is_some_and(|p| p.mats.contains_key(&key));
        let used = self.mats.users.contains_key(&key) || in_preview;
        if used {
            self.mats.force_tex.insert(key.clone());
            if let Some(e) = self.mats.analyses.get_mut(&key) {
                e.loading = true;
                e.queued = false;
            }
            self.request_analysis(&key, Lane::High);
        } else {
            self.mats.analyses.remove(&key);
        }
    }

    pub(crate) fn model_info_impl(&mut self, path: &str) -> ModelState {
        let key = model_key(path);
        match self.models.map.get(&key) {
            Some(ModelEntry::Ready(m)) => return ModelState::Ready(Arc::clone(&m.info)),
            Some(ModelEntry::Cached(info)) => return ModelState::Ready(Arc::clone(info)),
            Some(ModelEntry::Failed(e)) => return ModelState::Failed(tf!("assets.model_failed", e)),
            Some(ModelEntry::Loading) => {
                request_model_shared(&self.shared, &key);
                return ModelState::Loading;
            }
            None => {}
        }
        if key.is_empty() {
            return ModelState::Failed(tf!("assets.model_failed", path));
        }
        if self.fs.is_none() && self.status != AssetStatus::Mounting {
            return ModelState::Failed(t!("assets.no_game").to_owned());
        }
        self.models.map.insert(key.clone(), ModelEntry::Loading);
        request_model_again(&self.shared, &key);
        ModelState::Loading
    }

    pub(crate) fn model_data(&mut self, key: &str) -> Option<Arc<crate::models::PreparedModel>> {
        self.models.last_used.insert(key.to_owned(), self.shared.current_frame());
        match self.models.map.get(key) {
            Some(ModelEntry::Ready(m)) => Some(Arc::clone(m)),
            Some(ModelEntry::Failed(_)) => None,
            Some(ModelEntry::Loading) => {
                request_model_shared(&self.shared, key);
                None
            }
            None | Some(ModelEntry::Cached(_)) => {
                self.models.map.insert(key.to_owned(), ModelEntry::Loading);
                request_model_again(&self.shared, key);
                None
            }
        }
    }

    pub(crate) fn entity_bounds_impl(&self, e: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        if e.classname.eq_ignore_ascii_case("prop_ragdoll")
            && let Some(bounds) = self.ragdoll_bounds(e.id)
        {
            return Some(bounds);
        }
        let class = class_of(self.fgd.as_deref(), e);
        let origin = e.origin().filter(|o| o.is_finite()).unwrap_or(glam::DVec3::ZERO);
        if let Some(path) = model_path(e, class) {
            let key = model_key_cow(&path);
            let state = self.shared.bounds.read().get(key.as_ref()).copied();
            match state {
                Some(BoundsState::Ready { min, max }) => {
                    let angles = e.angles().filter(|a| a.is_finite()).unwrap_or(glam::DVec3::ZERO);
                    let m = entity_matrix(origin, angles, model_scale(e));
                    let (lo, hi) = (glam::Vec3::from(min).as_dvec3(), glam::Vec3::from(max).as_dvec3());
                    let b = transformed_bounds(lo, hi, &m);
                    if !b.is_empty() && b.size().max_element() > 0.0 && b.min.is_finite() && b.max.is_finite() {
                        return Some(b);
                    }
                }
                Some(_) => {}
                None => request_model_shared(&self.shared, &key),
            }
        }
        let (min, max) = local_bounds(class);
        Some(hp_geom::Aabb::new(origin + min, origin + max))
    }

    pub(crate) fn poll_results(&mut self) {
        while let Ok(r) = self.rx.try_recv() {
            self.handle_result(r);
        }
    }

    pub(crate) fn handle_result(&mut self, r: JobResult) {
        match r {
            JobResult::Material { name, epoch, hdr, mode_epoch, data } => {
                if epoch == self.mount_gen && hdr == self.mats.hdr && mode_epoch == self.mats.mode_epoch {
                    self.on_material(name, data);
                }
            }
            JobResult::Texture { name, target, epoch, data, held } => {
                if epoch == self.mount_gen {
                    self.uploads.push_back(Upload { name, target, epoch, data, _held: held });
                }
            }
            JobResult::Thumb { name, epoch, data } => self.on_thumb(name, epoch, data),
            JobResult::ThumbDropped { name } => self.thumbs.forget_pending(&name),
            JobResult::Model { path, epoch, data } => {
                if epoch == self.mount_gen || data.is_ok() {
                    self.on_model(path, data);
                } else {
                    self.models.map.remove(&path);
                    self.shared.bounds.write().remove(&path);
                }
            }
            JobResult::ModelLods { key, epoch, data } => {
                if epoch == self.mount_gen {
                    self.on_model_lods(key, data);
                }
            }
            JobResult::Mounted { ticket, outcome } => {
                if ticket == self.mount_ticket {
                    self.on_mounted(outcome);
                }
            }
            JobResult::MountedLate { ticket, added } => {
                if ticket == self.mount_ticket {
                    self.late_sources = 0;
                    self.generation += 1;
                    if added > 0 {
                        self.messages.push((hp_app::Severity::Info, tf!("assets.late_mounted", added)));
                    }
                }
            }
            JobResult::FgdLoaded { ticket, outcome } => {
                if ticket == self.fgd_ticket {
                    for w in &outcome.warnings {
                        log::warn!("FGD: {w}");
                    }
                    self.fgd_report = hp_app::FgdReport {
                        files: outcome.files.clone(),
                        warnings: outcome.warnings.clone(),
                        notes: outcome.notes.iter().take(300).cloned().collect(),
                        loading: false,
                    };
                    let lines = crate::mount::fgd_note_lines(&outcome.notes, FGD_NOTES_SHOWN);
                    self.messages.extend(lines.into_iter().map(|l| (hp_app::Severity::Warning, l)));
                    self.fgd_done = ticket;
                    self.fgd = outcome.fgd;
                    self.generation += 1;
                    self.scene.force_entities = true;
                }
            }
            JobResult::Lists { epoch, lists } => {
                if epoch == self.mount_gen {
                    self.lists_pending = false;
                    self.install_lists(lists);
                    self.generation += 1;
                    self.fresh.clear();
                    self.reload_all(false);
                }
            }
            JobResult::Refreshed { names } => {
                for n in names {
                    self.invalidate_material_impl(&n);
                }
                self.generation += 1;
            }
        }
    }

    fn on_material(&mut self, name: String, data: crate::material::MaterialData) {
        let force = self.mats.force_tex.remove(&name);
        self.mats.infos.insert(name.clone(), data.info);
        let entry = self.mats.analyses.entry(name.clone()).or_default();
        entry.value = Some(data.analysis.clone());
        entry.loading = false;
        entry.queued = false;
        let ids = self.mats.users.get(&name).cloned().unwrap_or_default();
        for id in ids {
            self.mats.mark_dirty(id);
            let cold = self.mats.records.get((id as usize).wrapping_sub(1)).is_some_and(|r| r.cold);
            if !cold && let Some(a) = &data.analysis {
                self.want_textures(id, a);
            }
        }
        if force && let Some(a) = &data.analysis {
            let ids = self.mats.users.get(&name).cloned().unwrap_or_default();
            let mut keys: Vec<String> = ids.iter().flat_map(|id| texture_keys(self.material_kind(*id), a)).collect();
            keys.sort_unstable();
            keys.dedup();
            for key in keys {
                self.queue_texture(&key, TexTarget::Main);
            }
        }
        self.preview_on_material(&name, data.analysis.as_deref(), force);
    }

    fn on_model(&mut self, path: String, data: Result<Arc<crate::models::PreparedModel>, String>) {
        match data {
            Ok(m) => {
                self.models.last_used.insert(path.clone(), self.shared.current_frame());
                self.models.map.insert(path.clone(), ModelEntry::Ready(m));
                self.models.newly_ready.insert(path.clone());
            }
            Err(e) => {
                self.models.map.insert(path.clone(), ModelEntry::Failed(e));
            }
        }
        if let Some(p) = &mut self.preview
            && p.model_path() == Some(path.as_str())
        {
            p.needs_render = true;
        }
    }

    pub(crate) fn start_mount(&mut self, settings: &hp_app::settings::Settings) {
        self.mount_ticket += 1;
        self.late_sources = 0;
        self.status = AssetStatus::Mounting;
        let ticket = self.mount_ticket;
        let req = MountRequest::from_settings(settings);
        let progress = Arc::new(hp_fs::MountProgress::new());
        self.mount_progress = Arc::clone(&progress);
        let ok = spawn_task(
            "hp-assets-mount",
            &self.shared,
            move || JobResult::Mounted { ticket, outcome: run_mount(&req, &progress) },
            move || JobResult::Mounted { ticket, outcome: MountOutcome::Failed("mount thread panicked".into()) },
        );
        if !ok {
            self.status = AssetStatus::Failed("cannot start the mount thread".into());
        }
    }

    fn start_late_mount(&mut self, rest: hp_fs::DeferredMount) {
        let Some(fs) = self.fs.clone() else { return };
        if rest.is_empty() {
            return;
        }
        self.late_sources = rest.len();
        let ticket = self.mount_ticket;
        let progress = Arc::new(hp_fs::MountProgress::new());
        progress.begin_opening(rest.len());
        self.late_progress = Arc::clone(&progress);
        let ok = spawn_task(
            "hp-assets-mount-late",
            &self.shared,
            move || JobResult::MountedLate { ticket, added: fs.finish_mount_with(rest, &progress) },
            move || JobResult::MountedLate { ticket, added: 0 },
        );
        if !ok {
            self.late_sources = 0;
        }
    }

    pub(crate) fn start_fgd(&mut self, settings: &hp_app::settings::Settings) {
        self.fgd_ticket += 1;
        let ticket = self.fgd_ticket;
        let req = FgdRequest::from_settings(settings);
        let ok = spawn_task(
            "hp-assets-fgd",
            &self.shared,
            move || JobResult::FgdLoaded { ticket, outcome: run_fgd(&req) },
            move || JobResult::FgdLoaded {
                ticket,
                outcome: crate::mount::FgdOutcome {
                    fgd: None,
                    warnings: vec!["FGD thread panicked".into()],
                    notes: Vec::new(),
                    files: Vec::new(),
                },
            },
        );
        if !ok {
            self.fgd_done = ticket;
        }
    }

    fn install_lists(&mut self, lists: AssetLists) {
        self.fs_gen_seen = lists.fs_gen;
        self.material_list = lists.materials;
        self.model_list = lists.models;
        self.sound_list = lists.sounds;
        self.labels = lists.labels.into_iter().collect();
    }

    fn on_mounted(&mut self, outcome: MountOutcome) {
        match outcome {
            MountOutcome::NotConfigured => {
                self.status = AssetStatus::NotConfigured;
                self.root = None;
                self.fs = None;
                self.mount_gen += 1;
                *self.shared.env.write() = None;
                self.material_list = Arc::new(Vec::new());
                self.model_list = Arc::new(Vec::new());
                self.sound_list = Arc::new(Vec::new());
                self.labels.clear();
                self.lists_pending = false;
                self.generation += 1;
                self.thumbs.clear();
                self.shared.clear_thumbs();
            }
            MountOutcome::Failed(msg) => {
                log::error!("mounting game content failed: {msg}");
                self.status = AssetStatus::Failed(msg);
                self.generation += 1;
            }
            MountOutcome::Mounted(data) => {
                let data = *data;
                let line = crate::mount::sources_line(&data.fs.sources(), MOUNT_SOURCES_SHOWN);
                self.messages.push((hp_app::Severity::Info, line));
                self.mount_gen += 1;
                *self.shared.env.write() = Some(FsEnv { fs: data.fs.clone(), epoch: self.mount_gen });
                self.root = Some(data.root);
                self.fs = Some(data.fs);
                self.install_lists(data.lists);
                self.lists_pending = false;
                self.status = AssetStatus::Ready;
                self.generation += 1;
                self.fresh.clear();
                self.uploads.clear();
                self.reload_all(true);
                self.start_late_mount(data.rest);
            }
        }
    }

    pub(crate) fn reload_all(&mut self, full: bool) {
        let names: Vec<String> = self.mats.analyses.keys().cloned().collect();
        for name in names {
            let used = self.mats.users.contains_key(&name)
                || self.preview.as_ref().is_some_and(|p| p.mats.contains_key(&name));
            if let Some(e) = self.mats.analyses.get_mut(&name) {
                e.loading = true;
                e.queued = false;
            }
            self.queue_analysis(&name, if used { Lane::High } else { Lane::Low });
        }
        let textures: Vec<String> = self
            .mats
            .textures
            .map
            .iter()
            .filter(|(_, e)| full || e.missing || (e.loading && !e.queued))
            .map(|(k, _)| k.clone())
            .collect();
        for t in textures {
            self.queue_texture(&t, TexTarget::Main);
        }
        let preview_textures: Vec<String> = self
            .preview
            .as_ref()
            .map(|p| {
                p.textures
                    .map
                    .iter()
                    .filter(|(_, e)| full || e.missing || (e.loading && !e.queued))
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default();
        for t in preview_textures {
            self.queue_texture(&t, TexTarget::Preview);
        }
        self.thumbs.clear();
        self.shared.clear_thumbs();
        self.models.map.retain(|_, m| !matches!(m, ModelEntry::Failed(_)));
        self.shared.bounds.write().retain(|_, b| *b != BoundsState::Failed);
        let waiting: Vec<String> =
            self.models.map.iter().filter(|(_, m)| matches!(m, ModelEntry::Loading)).map(|(k, _)| k.clone()).collect();
        for k in waiting {
            request_model_shared(&self.shared, &k);
        }
        self.scene.force_entities = true;
        if let Some(p) = &mut self.preview {
            p.needs_render = true;
        }
    }

    fn check_fs_changes(&mut self) {
        let Some(fs) = &self.fs else { return };
        if self.lists_pending || fs.generation() == self.fs_gen_seen {
            return;
        }
        self.lists_pending = true;
        let fs = fs.clone();
        let epoch = self.mount_gen;
        let fs2 = fs.clone();
        let ok = spawn_task(
            "hp-assets-lists",
            &self.shared,
            move || JobResult::Lists { epoch, lists: AssetLists::build(&fs) },
            move || JobResult::Lists {
                epoch,
                lists: AssetLists {
                    fs_gen: fs2.generation(),
                    materials: Arc::new(Vec::new()),
                    models: Arc::new(Vec::new()),
                    sounds: Arc::new(Vec::new()),
                    labels: Vec::new(),
                },
            },
        );
        if !ok {
            self.lists_pending = false;
        }
    }

    pub(crate) fn begin_frame_impl(&mut self, ctx: &egui::Context, gfx: Option<&mut Gfx>) {
        self.shared.set_ctx(ctx);
        self.shared.frame.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if let Some(gfx) = gfx.as_ref() {
            self.update_material_hdr(&gfx.renderer);
        }
        self.poll_results();
        self.check_fs_changes();
        self.update_watch();
        if let Some(gfx) = gfx {
            self.note_gpu(&gfx.renderer);
            self.poll_watcher(&mut gfx.renderer);
            self.process_uploads(gfx);
            self.mats.apply_dirty(&mut gfx.renderer);
            self.preview_apply_dirty();
            self.process_model_thumbs(gfx);
        }
        self.create_thumb_textures();
        self.trim_model_geometry();
        if !self.uploads.is_empty() {
            ctx.request_repaint();
        }
    }

    pub(crate) fn note_gpu(&self, renderer: &Renderer) {
        let v = if renderer.supports_bc() { crate::queue::BC_YES } else { crate::queue::BC_NO };
        self.shared.bc.store(v, std::sync::atomic::Ordering::Relaxed);
    }

    pub(crate) fn update_material_hdr(&mut self, renderer: &Renderer) {
        let baked = (renderer.lightmap_display() == hp_render::lightmap::LightmapDisplay::Baked
            && renderer.has_baked_lightmaps())
        .then(|| !renderer.lightmap_ldr());
        let hdr = self
            .play_world
            .material_hdr()
            .or(baked)
            .unwrap_or_else(|| hp_app::runtime::render::render_display().light_hdr);
        self.set_material_hdr(hdr);
    }

    fn set_material_hdr(&mut self, hdr: bool) {
        if self.mats.hdr == hdr {
            return;
        }
        self.mats.hdr = hdr;
        self.mats.mode_epoch = self.mats.mode_epoch.wrapping_add(1);
        self.mats.infos.clear();
        let names: Vec<String> = self.mats.analyses.keys().cloned().collect();
        for name in names {
            let used = self.mats.users.get(&name).is_some_and(|ids| {
                ids.iter().any(|id| self.mats.records.get((*id as usize).wrapping_sub(1)).is_some_and(|r| !r.cold))
            }) || self.preview.as_ref().is_some_and(|p| p.mats.contains_key(&name));
            if used {
                if let Some(e) = self.mats.analyses.get_mut(&name) {
                    e.loading = true;
                    e.queued = false;
                }
                self.queue_analysis(&name, Lane::High);
            } else {
                self.mats.analyses.remove(&name);
            }
        }
        self.generation = self.generation.wrapping_add(1);
    }

    pub(crate) fn process_uploads(&mut self, gfx: &mut Gfx) {
        let budget = Budget::new(if self.uploads.len() > BULK_UPLOADS { UPLOAD_BUDGET_BULK } else { UPLOAD_BUDGET });
        while let Some(up) = self.uploads.pop_front() {
            if up.epoch != self.mount_gen {
                continue;
            }
            let heavy = up.data.is_some();
            match up.target {
                TexTarget::Main => self.finish_texture_main(&mut gfx.renderer, up),
                TexTarget::Preview => self.finish_texture_preview(up),
            }
            if heavy && budget.exceeded() {
                break;
            }
        }
    }

    fn finish_texture_main(&mut self, renderer: &mut Renderer, up: Upload) {
        let Some(entry) = self.mats.textures.map.get_mut(&up.name) else { return };
        let new = up.data.and_then(|p| {
            let id = renderer.create_texture(&p.upload);
            (id != TexId::MISSING).then_some((id, p.size))
        });
        entry.loading = false;
        entry.queued = false;
        entry.missing = new.is_none();
        let old = std::mem::replace(&mut entry.current, new);
        if let Some((old_id, _)) = old {
            renderer.destroy_texture(old_id);
        }
        self.mats.mark_texture_users(&up.name);
    }
}

#[cfg(test)]
mod hdr_tests {
    use super::*;
    use crate::material::material_job;

    #[test]
    fn material_mode_changes_refresh_used_materials_and_reject_stale_results() {
        let dir = std::env::temp_dir().join(format!("hp-material-hdr-{}", std::process::id()));
        let materials = dir.join("materials/hp_hdr");
        std::fs::create_dir_all(&materials).unwrap();
        std::fs::write(
            materials.join("wall.vmt"),
            r#"LightmappedGeneric {
            $basetexture hp_hdr/ldr $bumpmap hp_hdr/normal_ldr $envmap env_cubemap
            $envmaptint "[.8 .7 .6]"
            LightmappedGeneric_HDR_DX9 {
                $basetexture hp_hdr/hdr $bumpmap hp_hdr/normal_hdr $envmaptint "[.2 .3 .4]"
            }
        }"#,
        )
        .unwrap();
        let fs = hp_fs::GameFs::empty();
        fs.add_game_dir(&dir).unwrap();
        let name = "hp_hdr/wall";
        let mut am = AssetManager::new(&hp_app::settings::Settings::default());
        am.mount_ticket += 1;
        am.fgd_ticket += 1;
        *am.shared.env.write() = None;
        am.mats.hdr = false;
        am.asset_material(name, MatKind::World);
        am.on_material(name.to_owned(), material_job(&fs, name, false, false));
        let AnaLook::Ready(before) = am.mats.ana_look(name) else { panic!("LDR ready") };
        assert_eq!(before.texture.as_deref(), Some("hp_hdr/ldr"));
        let before = before.clone();
        let generation = am.generation;
        am.set_material_hdr(true);
        assert!(am.mats.hdr && am.mats.infos.is_empty());
        assert_eq!(am.generation, generation + 1);
        assert!(am.mats.analyses[name].loading);
        let AnaLook::Ready(visible) = am.mats.ana_look(name) else { panic!("keep displayed material during load") };
        assert_eq!(&before, visible);
        am.handle_result(JobResult::Material {
            name: name.to_owned(),
            epoch: am.mount_gen,
            hdr: false,
            mode_epoch: 0,
            data: material_job(&fs, name, false, false),
        });
        assert!(am.mats.infos.is_empty() && am.mats.analyses[name].loading, "stale LDR cannot finish the HDR request");
        am.handle_result(JobResult::Material {
            name: name.to_owned(),
            epoch: am.mount_gen,
            hdr: true,
            mode_epoch: am.mats.mode_epoch,
            data: material_job(&fs, name, false, true),
        });
        let AnaLook::Ready(hdr) = am.mats.ana_look(name) else { panic!("HDR ready") };
        assert_eq!(hdr.texture.as_deref(), Some("hp_hdr/hdr"));
        assert_eq!(hdr.bump.as_ref().unwrap().texture, "hp_hdr/normal_hdr");
        assert_eq!(hdr.envmap.as_ref().unwrap().tint, [0.2, 0.3, 0.4]);
        assert_eq!(am.mats.infos[name].base_texture.as_deref(), Some("hp_hdr/hdr"));
        am.set_material_hdr(true);
        assert!(!am.mats.analyses[name].loading, "unchanged HDR does not reload every frame");
        am.set_material_hdr(false);
        am.handle_result(JobResult::Material {
            name: name.to_owned(),
            epoch: am.mount_gen,
            hdr: true,
            mode_epoch: am.mats.mode_epoch - 1,
            data: material_job(&fs, name, false, true),
        });
        assert!(am.mats.infos.is_empty(), "stale HDR cannot replace the new LDR selection");
        am.handle_result(JobResult::Material {
            name: name.to_owned(),
            epoch: am.mount_gen,
            hdr: false,
            mode_epoch: am.mats.mode_epoch,
            data: material_job(&fs, name, false, false),
        });
        let AnaLook::Ready(ldr) = am.mats.ana_look(name) else { panic!("LDR restored") };
        assert_eq!(ldr.bump.as_ref().unwrap().texture, "hp_hdr/normal_ldr");
        assert_eq!(ldr.envmap.as_ref().unwrap().tint, [0.8, 0.7, 0.6]);
        let old_epoch = am.mats.mode_epoch;
        am.set_material_hdr(true);
        am.set_material_hdr(false);
        am.handle_result(JobResult::Material {
            name: name.to_owned(),
            epoch: am.mount_gen,
            hdr: false,
            mode_epoch: old_epoch,
            data: material_job(&fs, name, false, false),
        });
        assert!(
            am.mats.infos.is_empty() && am.mats.analyses[name].loading,
            "LDR from an earlier round cannot complete the new LDR request"
        );
        drop(am);
        let _ = std::fs::remove_dir_all(dir);
    }
}
