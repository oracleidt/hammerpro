use std::sync::Arc;

use ahash::{AHashMap, AHashSet};
use glam::Vec3;
use hp_app::state::{Gfx, PreviewCamera};
use hp_mdl::{AnimClip, Bone, BoneWeight, ModelVertex, SkinMatrix};
use hp_render::{
    GridParams, MaterialId, ModelId, ModelInstance, ModelPart, ObjectMesh, Overlay, RenderMode, Renderer, Vertex,
    ViewKind, ViewParams,
};

use crate::AssetManager;
use crate::material::{AnaLook, MatAnalysis, MatKind, asset_desc, texture_keys};
use crate::models::PreparedModel;
use crate::queue::Lane;
use crate::texture::{TexRegistry, TexTarget};
use crate::util::model_key;

mod geometry;

const VIEW: u32 = 0;
const MODEL_OBJECT: u32 = 1;
const WIRE_OBJECT: u32 = 2;
const FOV_DEG: f32 = 40.0;
const MAX_KEPT_MATERIALS: usize = 1024;
pub(crate) const BACKGROUND: [f32; 3] = [0.15, 0.16, 0.18];
const WIRE_COLOR: [u8; 3] = [255, 190, 110];

struct PreviewModel {
    path: String,
    skin: u32,
    id: Option<ModelId>,
    bounds: (Vec3, Vec3),
    data: std::sync::Arc<PreparedModel>,
    wire: bool,
}

struct AnimPart {
    material: MaterialId,
    bind: Vec<ModelVertex>,
    weights: Vec<BoneWeight>,
    indices: Vec<u32>,
}

struct AnimBase {
    path: String,
    skin: u32,
    sequence: usize,
    mdl: Arc<Vec<u8>>,
    ani: Option<Arc<Vec<u8>>>,
    bones: Vec<Bone>,
    parts: Vec<AnimPart>,
    clip: Option<AnimClip>,
    posed: Option<(usize, u32)>,
}

pub(crate) struct Preview {
    pub renderer: Renderer,
    pub textures: TexRegistry,
    tex_users: AHashMap<String, Vec<MaterialId>>,
    pub mats: AHashMap<String, MaterialId>,
    names: Vec<String>,
    dirty: Vec<MaterialId>,
    model: Option<PreviewModel>,
    anim: Option<AnimBase>,
    anim_id: Option<ModelId>,
    anim_failed: Option<(String, u32, usize)>,
    egui: Option<(egui::TextureId, u64)>,
    last: Option<(String, PreviewCamera, [u32; 2])>,
    pub needs_render: bool,
    normal_maps: bool,
    geometry: geometry::GeometryPreview,
}

impl Preview {
    pub(crate) fn model_id(&self) -> Option<ModelId> {
        self.model.as_ref().and_then(|m| m.id)
    }

    pub(crate) fn new(gfx: &Gfx) -> Self {
        Self {
            renderer: Renderer::new(&gfx.device, &gfx.queue),
            textures: TexRegistry::default(),
            tex_users: AHashMap::new(),
            mats: AHashMap::new(),
            names: Vec::new(),
            dirty: Vec::new(),
            model: None,
            anim: None,
            anim_id: None,
            anim_failed: None,
            egui: None,
            last: None,
            needs_render: true,
            normal_maps: hp_app::runtime::render::render_display().normal_maps,
            geometry: geometry::GeometryPreview::default(),
        }
    }

    fn show_static_model(&mut self) {
        match self.model.as_ref().and_then(|m| m.id) {
            Some(id) => self.renderer.set_model_instances(&[model_instance(id)]),
            None => self.renderer.set_model_instances(&[]),
        }
    }

    fn stop_anim(&mut self) {
        if let Some(id) = self.anim_id.take() {
            self.renderer.destroy_model(id);
        }
        if self.anim.take().is_some() {
            self.show_static_model();
            self.needs_render = true;
        }
    }

    fn pose_anim(&mut self, time_s: f32) -> bool {
        let Some(base) = self.anim.as_ref() else { return false };
        let (frame, frac) = base.clip.map_or((0, 0.0), |c| c.frame_at(time_s));
        if base.posed == Some((frame, frac.to_bits())) && self.anim_id.is_some() {
            return true;
        }
        let Some(matrices) = hp_mdl::skin_matrices_with(
            &base.mdl,
            base.ani.as_deref().map(Vec::as_slice),
            &base.bones,
            base.sequence,
            frame,
            frac,
        ) else {
            return false;
        };
        let parts: Vec<ModelPart> = base
            .parts
            .iter()
            .map(|p| ModelPart {
                material: p.material,
                vertices: skin_part(&matrices, &p.bind, &p.weights),
                indices: p.indices.clone(),
                tangents: Vec::new(),
            })
            .collect();
        if let Some(id) = self.anim_id.take() {
            self.renderer.destroy_model(id);
        }
        let id = self.renderer.create_model(&parts);
        self.anim_id = Some(id);
        if let Some(base) = self.anim.as_mut() {
            base.posed = Some((frame, frac.to_bits()));
        }
        self.renderer.set_model_instances(&[model_instance(id)]);
        self.needs_render = true;
        true
    }

    pub(crate) fn hide_for_thumbnail(&mut self) {
        self.renderer.remove_object(WIRE_OBJECT);
        if let Some(m) = self.model.as_mut() {
            m.wire = false;
        }
    }

    pub(crate) fn restore_after_thumbnail(&mut self) {
        match self.anim_id {
            Some(id) => self.renderer.set_model_instances(&[model_instance(id)]),
            None => self.show_static_model(),
        }
    }

    pub(crate) fn model_path(&self) -> Option<&str> {
        self.model.as_ref().map(|m| m.path.as_str())
    }

    fn mark_dirty(&mut self, id: MaterialId) {
        if !self.dirty.contains(&id) {
            self.dirty.push(id);
        }
    }

    fn reset_materials(&mut self) {
        self.geometry.invalidate();
        for e in self.textures.map.values() {
            if let Some((id, _)) = e.current {
                self.renderer.destroy_texture(id);
            }
        }
        self.textures.map.clear();
        self.tex_users.clear();
        self.mats.clear();
        self.names.clear();
        self.dirty.clear();
    }
}

fn model_instance(id: ModelId) -> ModelInstance {
    ModelInstance {
        model: id,
        transform: glam::Mat4::IDENTITY.to_cols_array_2d(),
        object: MODEL_OBJECT,
        tint: [1.0; 4],
        fade: [0.0; 2],
    }
}

pub(crate) fn skin_part(matrices: &[SkinMatrix], bind: &[ModelVertex], weights: &[BoneWeight]) -> Vec<Vertex> {
    bind.iter()
        .enumerate()
        .map(|(i, v)| match weights.get(i) {
            Some(w) => {
                let (pos, normal) = hp_mdl::pose_vertex(matrices, w, v.pos, v.normal);
                Vertex { pos, normal, uv: v.uv, alpha: 1.0 }
            }
            None => Vertex { pos: v.pos, normal: v.normal, uv: v.uv, alpha: 1.0 },
        })
        .collect()
}

fn wire_mesh(data: &PreparedModel) -> ObjectMesh {
    let mut edges = Vec::new();
    for part in &data.parts {
        let mut seen: AHashSet<(u32, u32)> = AHashSet::with_capacity(part.indices.len());
        for tri in part.indices.chunks_exact(3) {
            for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                let key = (a.min(b), a.max(b));
                if !seen.insert(key) {
                    continue;
                }
                if let (Some(va), Some(vb)) = (part.vertices.get(a as usize), part.vertices.get(b as usize)) {
                    edges.push(va.pos);
                    edges.push(vb.pos);
                }
            }
        }
    }
    let (lo, hi) = data.bounds;
    ObjectMesh { batches: Vec::new(), edges, aabb_min: lo, aabb_max: hi, wire_color: WIRE_COLOR, ..Default::default() }
}

const BBOX_COLOR: [f32; 4] = [0.43, 0.78, 1.0, 1.0];
const HULL_COLOR: [f32; 4] = [1.0, 0.9, 0.31, 1.0];
const HITBOX_COLOR: [f32; 4] = [1.0, 0.35, 0.35, 1.0];
const SEQ_BOX_COLOR: [f32; 4] = [0.75, 0.45, 1.0, 1.0];
const BONE_COLOR: [f32; 4] = [0.95, 0.95, 0.95, 1.0];
const JOINT_COLOR: [f32; 4] = [1.0, 0.55, 0.1, 1.0];
const ATTACHMENT_COLOR: [f32; 4] = [0.3, 1.0, 0.6, 1.0];
const NORMAL_COLOR: [f32; 4] = [0.4, 0.6, 1.0, 0.8];
const ILLUM_COLOR: [f32; 4] = [1.0, 0.85, 0.2, 1.0];
const GROUND_COLOR: [f32; 4] = [0.55, 0.55, 0.6, 0.5];
const MAX_NORMALS: usize = 40_000;

fn helpers_overlay(data: &PreparedModel, camera: &PreviewCamera) -> Overlay {
    use hp_app::state::preview_overlay as po;
    use hp_render::{OverlayLine, OverlayPoint};
    let mut out = Overlay::default();
    let (lo, hi) = data.bounds;
    let radius = ((Vec3::from(hi) - Vec3::from(lo)).length() * 0.5).max(1.0);
    let line = |out: &mut Vec<OverlayLine>, a: [f32; 3], b: [f32; 3], color: [f32; 4]| {
        out.push(OverlayLine { a, b, color });
    };
    let boxed = |out: &mut Vec<OverlayLine>, a: [f32; 3], b: [f32; 3], color: [f32; 4]| {
        if a.iter().chain(b.iter()).all(|v| v.is_finite()) {
            for (p, q) in crate::models::box_edges(a, b) {
                out.push(OverlayLine { a: p, b: q, color });
            }
        }
    };
    let h = &data.helpers;
    if camera.shows(po::GROUND) {
        let step = (radius / 4.0).max(1.0);
        let n = 8;
        let ext = step * n as f32;
        for i in -n..=n {
            let v = step * i as f32;
            line(&mut out.lines, [v, -ext, 0.0], [v, ext, 0.0], GROUND_COLOR);
            line(&mut out.lines, [-ext, v, 0.0], [ext, v, 0.0], GROUND_COLOR);
        }
    }
    if camera.shows(po::BBOX) {
        boxed(&mut out.lines_on_top, lo, hi, BBOX_COLOR);
    }
    if camera.shows(po::HULL)
        && let (Some(a), Some(b)) = (data.info.hull_min, data.info.hull_max)
    {
        boxed(&mut out.lines_on_top, a.as_vec3().to_array(), b.as_vec3().to_array(), HULL_COLOR);
    }
    if camera.shows(po::HITBOXES) {
        for pair in h.hitbox_edges.chunks_exact(2) {
            line(&mut out.lines_on_top, pair[0], pair[1], HITBOX_COLOR);
        }
    }
    if camera.shows(po::SEQ_BOX)
        && let Some((a, b)) = h.seq_boxes.get(camera.anim.unwrap_or(0))
    {
        boxed(&mut out.lines_on_top, *a, *b, SEQ_BOX_COLOR);
    }
    if camera.shows(po::BONES) {
        for pair in h.bone_lines.chunks_exact(2) {
            line(&mut out.lines_on_top, pair[0], pair[1], BONE_COLOR);
        }
        for p in &h.joints {
            out.points.push(OverlayPoint { pos: *p, color: JOINT_COLOR, size_px: 5.0 });
        }
    }
    if camera.shows(po::ATTACHMENTS) {
        for p in &h.attachments {
            out.points.push(OverlayPoint { pos: *p, color: ATTACHMENT_COLOR, size_px: 7.0 });
        }
    }
    if camera.shows(po::NORMALS) {
        let len = radius * 0.04;
        for v in data.parts.iter().flat_map(|p| &p.vertices).take(MAX_NORMALS) {
            let n = Vec3::from(v.normal);
            if n.is_finite() && n.length_squared() > 0.0 {
                let a = Vec3::from(v.pos);
                line(&mut out.lines, a.to_array(), (a + n.normalize() * len).to_array(), NORMAL_COLOR);
            }
        }
    }
    if camera.shows(po::ORIGIN) {
        let len = radius * 0.35;
        line(&mut out.lines_on_top, [0.0; 3], [len, 0.0, 0.0], [1.0, 0.25, 0.25, 1.0]);
        line(&mut out.lines_on_top, [0.0; 3], [0.0, len, 0.0], [0.3, 1.0, 0.3, 1.0]);
        line(&mut out.lines_on_top, [0.0; 3], [0.0, 0.0, len], [0.35, 0.55, 1.0, 1.0]);
    }
    if camera.shows(po::ILLUM) && h.illum.iter().all(|v| v.is_finite()) {
        let c = Vec3::from(h.illum);
        let r = radius * 0.06;
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            line(&mut out.lines_on_top, (c - axis * r).to_array(), (c + axis * r).to_array(), ILLUM_COLOR);
        }
    }
    out
}

pub(crate) fn orbit_camera(min: Vec3, max: Vec3, cam: &PreviewCamera) -> ViewKind {
    let center = (min + max) * 0.5;
    let radius = ((max - min).length() * 0.5).max(1.0);
    let zoom = if cam.zoom.is_finite() { cam.zoom.clamp(0.05, 50.0) } else { 1.0 };
    let fov = if cam.fov_deg.is_finite() { cam.fov_deg.clamp(10.0, 120.0) } else { FOV_DEG };
    let dist = radius / (fov.to_radians() * 0.5).sin() * zoom;
    let yaw = if cam.yaw_deg.is_finite() { cam.yaw_deg.to_radians() } else { 0.0 };
    let pitch = if cam.pitch_deg.is_finite() { cam.pitch_deg.clamp(-89.0, 89.0).to_radians() } else { 0.0 };
    let forward = Vec3::new(pitch.cos() * yaw.cos(), pitch.cos() * yaw.sin(), -pitch.sin());
    let eye = center - forward * dist;
    ViewKind::Perspective {
        eye: eye.to_array(),
        forward: forward.to_array(),
        up: [0.0, 0.0, 1.0],
        fov_y_deg: fov,
        near: (dist * 0.01).max(0.1),
        far: dist + radius * 4.0,
    }
}

impl AssetManager {
    pub(crate) fn model_preview_impl(
        &mut self,
        gfx: &mut Gfx,
        path: &str,
        camera: &PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        let key = model_key(path);
        self.note_gpu(&gfx.renderer);
        self.poll_results();
        if self.preview.is_none() {
            self.preview = Some(Preview::new(gfx));
        }
        let data = self.model_data(&key)?;
        let thumbs_busy = self.model_thumbs_busy();
        let skin = if (camera.skin as usize) < data.skin_count() { camera.skin } else { 0 };
        let current = self.preview.as_ref().and_then(|p| p.model.as_ref()).map(|m| (m.path.clone(), m.skin));
        if current.as_ref().map(|(p, _)| p.as_str()) != Some(key.as_str()) {
            if let Some(p) = self.preview.as_mut() {
                p.stop_anim();
                if let Some(id) = p.model.take().and_then(|m| m.id) {
                    p.renderer.destroy_model(id);
                }
                if p.names.len() > MAX_KEPT_MATERIALS && !thumbs_busy {
                    p.reset_materials();
                }
            }
            self.preview_build_model(&key, skin, data.clone());
        } else if current.map(|(_, s)| s) != Some(skin) {
            if let Some(p) = self.preview.as_mut() {
                p.stop_anim();
                if let Some(id) = p.model.take().and_then(|m| m.id) {
                    p.renderer.destroy_model(id);
                }
            }
            self.preview_build_model(&key, skin, data.clone());
        }
        self.preview_sync_normal_maps();
        self.preview_apply_dirty();
        self.preview_collision(&key, camera.shows(hp_app::state::preview_overlay::PHYSICS));
        self.preview_animate(&key, skin, camera, &data);

        let p = self.preview.as_mut()?;
        let size = [size_px[0].clamp(16, 2048), size_px[1].clamp(16, 2048)];
        if let Some(m) = p.model.as_mut()
            && m.wire != camera.wireframe
        {
            m.wire = camera.wireframe;
            if m.wire {
                p.renderer.upsert_object(WIRE_OBJECT, wire_mesh(&m.data));
            } else {
                p.renderer.remove_object(WIRE_OBJECT);
            }
            p.needs_render = true;
        }
        let state = (key, *camera, size);
        if p.needs_render || p.last.as_ref() != Some(&state) {
            let (min, max) = p.model.as_ref().map_or((Vec3::splat(-16.0), Vec3::splat(16.0)), |m| m.bounds);
            let params = ViewParams {
                size,
                kind: orbit_camera(min, max, camera),
                mode: if camera.wireframe { RenderMode::TexturedWire } else { RenderMode::Textured },
                grid: None::<GridParams>,
                background: BACKGROUND,
                selection_color: [1.0, 0.3, 0.3, 0.0],
                show_sprites: false,
                show_models: true,
                msaa: true,
                game_pixel_grid: false,
            };
            let overlay = p.model.as_ref().map(|m| helpers_overlay(&m.data, camera)).unwrap_or_default();
            p.renderer.render_view(VIEW, &params, &overlay);
            p.last = Some(state);
            p.needs_render = false;
        }
        let generation = p.renderer.view_texture_generation(VIEW);
        let view = p.renderer.view_texture(VIEW)?;
        match p.egui {
            Some((id, g)) if g == generation => Some(id),
            Some((id, _)) => {
                gfx.update_texture(id, view);
                p.egui = Some((id, generation));
                Some(id)
            }
            None => {
                let id = gfx.register_texture(view);
                p.egui = Some((id, generation));
                Some(id)
            }
        }
    }

    fn preview_build_model(&mut self, key: &str, skin: u32, data: std::sync::Arc<PreparedModel>) {
        let mut parts = Vec::with_capacity(data.parts.len());
        for part in &data.parts {
            let name = data.part_material(skin as usize, part.skin_ref).to_owned();
            let material = self.preview_material(&name);
            parts.push(ModelPart {
                material,
                vertices: part.vertices.clone(),
                indices: part.indices.clone(),
                tangents: part.tangents.clone(),
            });
        }
        let Some(p) = self.preview.as_mut() else { return };
        let id = (!parts.is_empty()).then(|| p.renderer.create_model(&parts));
        if let Some(id) = id {
            p.renderer.set_model_instances(&[ModelInstance {
                model: id,
                transform: glam::Mat4::IDENTITY.to_cols_array_2d(),
                object: MODEL_OBJECT,
                tint: [1.0; 4],
                fade: [0.0; 2],
            }]);
        } else {
            p.renderer.set_model_instances(&[]);
        }
        p.renderer.remove_object(WIRE_OBJECT);
        let (lo, hi) = data.bounds;
        p.model = Some(PreviewModel {
            path: key.to_owned(),
            skin,
            id,
            bounds: (Vec3::from(lo), Vec3::from(hi)),
            data,
            wire: false,
        });
        p.last = None;
        p.needs_render = true;
    }

    fn preview_animate(&mut self, key: &str, skin: u32, camera: &PreviewCamera, data: &Arc<PreparedModel>) {
        let Some(seq) = camera.anim else {
            if let Some(p) = self.preview.as_mut() {
                p.stop_anim();
            }
            return;
        };
        let matches = self
            .preview
            .as_ref()
            .and_then(|p| p.anim.as_ref())
            .is_some_and(|a| a.path == key && a.skin == skin && a.sequence == seq);
        let failed = (key.to_owned(), skin, seq);
        if !matches {
            if let Some(p) = self.preview.as_mut() {
                p.stop_anim();
                if p.anim_failed.as_ref() == Some(&failed) {
                    return;
                }
            }
            if !self.preview_build_anim(key, skin, seq, data) {
                if let Some(p) = self.preview.as_mut() {
                    p.anim_failed = Some(failed);
                }
                return;
            }
        }
        if let Some(p) = self.preview.as_mut()
            && !p.pose_anim(camera.anim_time)
        {
            p.stop_anim();
            p.anim_failed = Some(failed);
        }
    }

    fn preview_build_anim(&mut self, key: &str, skin: u32, seq: usize, data: &Arc<PreparedModel>) -> bool {
        let opts =
            hp_mdl::LoadOptions { pose: hp_mdl::Pose::Bind, bone_weights: true, ..hp_mdl::LoadOptions::default() };
        let loaded = {
            let Some(fs) = self.fs.as_ref() else { return false };
            let Ok(model) = hp_mdl::load_from(key, |p| fs.read(p), &opts) else { return false };
            let Some(mdl) = fs.read(key) else { return false };
            let ani = hp_mdl::animation_block_path(&mdl).and_then(|p| fs.read(&p)).map(Arc::new);
            (model, mdl, ani)
        };
        let (model, mdl, ani) = loaded;
        if model.bones.is_empty() {
            return false;
        }
        let clip = hp_mdl::animation_clip(&mdl, seq);
        let mut parts = Vec::with_capacity(model.meshes.len());
        for mesh in &model.meshes {
            if mesh.indices.is_empty() || mesh.bone_weights.len() != mesh.vertices.len() {
                continue;
            }
            let name = data.part_material(skin as usize, mesh.skin_ref).to_owned();
            let material = self.preview_material(&name);
            parts.push(AnimPart {
                material,
                bind: mesh.vertices.clone(),
                weights: mesh.bone_weights.clone(),
                indices: mesh.indices.clone(),
            });
        }
        if parts.is_empty() {
            return false;
        }
        let base = AnimBase {
            path: key.to_owned(),
            skin,
            sequence: seq,
            mdl: Arc::new(mdl),
            ani,
            bones: model.bones,
            parts,
            clip,
            posed: None,
        };
        if let Some(p) = self.preview.as_mut() {
            p.renderer.remove_object(WIRE_OBJECT);
            p.anim = Some(base);
        }
        true
    }

    pub(crate) fn preview_material(&mut self, name: &str) -> MaterialId {
        let Some(p) = self.preview.as_mut() else { return 0 };
        if let Some(&id) = p.mats.get(name) {
            return id;
        }
        let id = MaterialId::try_from(p.names.len() + 1).unwrap_or(MaterialId::MAX);
        p.names.push(name.to_owned());
        p.mats.insert(name.to_owned(), id);
        p.mark_dirty(id);
        self.request_analysis(name, Lane::Thumb);
        if let AnaLook::Ready(a) = self.mats.ana_look(name) {
            let a = a.clone();
            self.preview_want_textures(id, &a, false);
        }
        id
    }

    fn preview_want_textures(&mut self, id: MaterialId, a: &MatAnalysis, force: bool) {
        let mut queue = Vec::new();
        if let Some(p) = self.preview.as_mut() {
            let reflection =
                |k: &str| k.ends_with(crate::envmap::CUBE_SUFFIX) || crate::envmap::parse_mask_key(k).is_some();
            for tex in texture_keys(MatKind::Model, a).into_iter().filter(|k| !reflection(k)) {
                let users = p.tex_users.entry(tex.clone()).or_default();
                if !users.contains(&id) {
                    users.push(id);
                }
                if p.textures.request(&tex) || force {
                    queue.push(tex);
                }
            }
        }
        for t in queue {
            self.queue_texture(&t, TexTarget::Preview);
        }
    }

    pub(crate) fn preview_sync_normal_maps(&mut self) {
        let on = hp_app::runtime::render::render_display().normal_maps;
        let Some(p) = self.preview.as_mut() else { return };
        if p.normal_maps == on {
            return;
        }
        p.normal_maps = on;
        p.needs_render = true;
        let mut users: Vec<(MaterialId, MatAnalysis)> = Vec::new();
        for (i, name) in p.names.iter().enumerate() {
            if let AnaLook::Ready(a) = self.mats.ana_look(name)
                && a.bump.is_some()
            {
                users.push((MaterialId::try_from(i + 1).unwrap_or(MaterialId::MAX), a.clone()));
            }
        }
        for (id, a) in &users {
            if on {
                self.preview_want_textures(*id, a, false);
            }
            if let Some(p) = self.preview.as_mut() {
                p.mark_dirty(*id);
            }
        }
        if !on && let Some(p) = self.preview.as_mut() {
            let normal: Vec<String> =
                p.textures.map.keys().filter(|k| crate::bump::parse_normal_key(k).is_some()).cloned().collect();
            for key in normal {
                if let Some(e) = p.textures.map.remove(&key)
                    && let Some((id, _)) = e.current
                {
                    p.renderer.destroy_texture(id);
                }
                p.tex_users.remove(&key);
            }
        }
        self.reset_model_thumbs();
    }

    pub(crate) fn preview_on_material(&mut self, name: &str, analysis: Option<&MatAnalysis>, force: bool) {
        let Some(id) = self.preview.as_ref().and_then(|p| p.mats.get(name).copied()) else { return };
        if let Some(p) = self.preview.as_mut() {
            p.mark_dirty(id);
        }
        if let Some(a) = analysis {
            self.preview_want_textures(id, a, force);
        }
    }

    pub(crate) fn finish_texture_preview(&mut self, up: crate::Upload) {
        let Some(p) = self.preview.as_mut() else { return };
        let Some(entry) = p.textures.map.get_mut(&up.name) else { return };
        let new = up.data.and_then(|d| {
            let id = p.renderer.create_texture(&d.upload);
            (id != hp_render::TexId::MISSING).then_some((id, d.size))
        });
        entry.loading = false;
        entry.queued = false;
        entry.missing = new.is_none();
        if let Some((old, _)) = std::mem::replace(&mut entry.current, new) {
            p.renderer.destroy_texture(old);
        }
        let users = p.tex_users.get(&up.name).cloned().unwrap_or_default();
        for id in users {
            p.mark_dirty(id);
        }
    }

    pub(crate) fn preview_apply_dirty(&mut self) {
        let Some(p) = self.preview.as_mut() else { return };
        if p.dirty.is_empty() {
            return;
        }
        let dirty = std::mem::take(&mut p.dirty);
        for id in dirty {
            let Some(name) = p.names.get((id as usize).wrapping_sub(1)) else { continue };
            let textures = &p.textures;
            let desc = asset_desc(MatKind::Model, name, self.mats.ana_look(name), &|t| textures.look(t), true);
            let desc = hp_render::MaterialDesc { envmap: None, ..desc };
            p.renderer.set_material(id, &desc);
        }
        // Each render target must keep its own invalidation. The model browser
        // may render first and consume the shared needs_render flag this frame.
        p.geometry.redraw();
        p.needs_render = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_frames_the_box() {
        let cam = PreviewCamera { yaw_deg: 0.0, pitch_deg: 0.0, zoom: 1.0, ..PreviewCamera::default() };
        let ViewKind::Perspective { eye, forward, near, far, .. } =
            orbit_camera(Vec3::splat(-10.0), Vec3::splat(10.0), &cam)
        else {
            panic!("perspective expected");
        };
        assert!((Vec3::from(forward) - Vec3::X).length() < 1e-5);
        assert!(eye[0] < -30.0 && eye[1].abs() < 1e-3, "{eye:?}");
        assert!(near > 0.0 && far > -eye[0]);
        let top = PreviewCamera { pitch_deg: 89.0, zoom: 2.0, ..cam };
        let ViewKind::Perspective { eye, .. } = orbit_camera(Vec3::splat(-10.0), Vec3::splat(10.0), &top) else {
            panic!("perspective expected");
        };
        assert!(eye[2] > 60.0, "{eye:?}");
    }

    #[test]
    fn wireframe_edges_are_unique() {
        use hp_render::Vertex;
        let v = |x: f32, y: f32| Vertex { pos: [x, y, 0.0], normal: [0.0, 0.0, 1.0], uv: [0.0; 2], alpha: 1.0 };
        let data = PreparedModel {
            info: std::sync::Arc::new(hp_app::state::ModelInfo::default()),
            parts: vec![crate::models::PreparedPart {
                skin_ref: 0,
                body_part: 0,
                choice: 0,
                vertices: vec![v(0.0, 0.0), v(1.0, 0.0), v(1.0, 1.0), v(0.0, 1.0)],
                indices: vec![0, 1, 2, 0, 2, 3],
                tangents: Vec::new(),
                color_index: Vec::new(),
            }],
            variant_parts: Vec::new(),
            body_parts: Vec::new(),
            skin_materials: vec![vec!["a".into()]],
            bounds: ([0.0; 3], [1.0, 1.0, 0.0]),
            helpers: Default::default(),
            lod_switch: Vec::new(),
            ..Default::default()
        };
        let m = wire_mesh(&data);
        assert_eq!(m.edges.len(), 5 * 2, "4 sides and the diagonal");
        assert!(m.batches.is_empty());
    }
}
