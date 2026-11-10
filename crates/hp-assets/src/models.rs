use std::sync::Arc;

use ahash::{AHashMap, AHashSet};
use glam::{DMat4, DQuat, DVec3};
use hp_app::state::ModelInfo;
use hp_render::{ModelId, Vertex};

pub(crate) struct PreparedPart {
    pub skin_ref: usize,
    pub body_part: usize,
    pub choice: usize,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub tangents: Vec<[f32; 4]>,
    pub color_index: Vec<u32>,
}

#[derive(Default)]
pub(crate) struct PreparedModel {
    pub info: Arc<ModelInfo>,
    pub parts: Vec<PreparedPart>,
    pub variant_parts: Vec<PreparedPart>,
    pub body_parts: Vec<(i32, u32)>,
    pub skin_materials: Vec<Vec<String>>,
    pub bounds: ([f32; 3], [f32; 3]),
    pub helpers: PreviewHelpers,
    pub lod_switch: Vec<f32>,
    pub particle_sets: Vec<hp_mdl::HitboxSet>,
    pub particle_bones: Vec<hp_mdl::SkinMatrix>,
    pub bind_bones: Vec<hp_mdl::SkinMatrix>,
    pub particle_animation: Option<Arc<crate::particle_model_draw::AnimationSource>>,
}

impl PreparedModel {
    pub(crate) fn geometry_bytes(&self) -> usize {
        self.parts
            .iter()
            .chain(&self.variant_parts)
            .map(|p| {
                p.vertices.capacity() * size_of::<Vertex>()
                    + p.indices.capacity() * size_of::<u32>()
                    + p.tangents.capacity() * size_of::<[f32; 4]>()
                    + p.color_index.capacity() * size_of::<u32>()
            })
            .sum::<usize>()
            + self.particle_animation.as_ref().map_or(0, |source| source.bytes())
    }

    pub(crate) fn skin_count(&self) -> usize {
        self.skin_materials.len().max(1)
    }

    pub(crate) fn body_choices(&self, body: u32) -> Vec<u32> {
        body_choices(&self.body_parts, body)
    }

    pub(crate) fn body_key(&self, body: u32) -> u32 {
        body_key(&self.body_parts, body)
    }

    pub(crate) fn parts_for_body(&self, body: u32) -> Vec<&PreparedPart> {
        let choices = self.body_choices(body);
        let wanted = |p: &PreparedPart| choices.get(p.body_part).copied().unwrap_or(0) as usize == p.choice;
        self.parts.iter().chain(&self.variant_parts).filter(|p| wanted(p)).collect()
    }

    pub(crate) fn part_material(&self, skin: usize, skin_ref: usize) -> &str {
        self.skin_materials
            .get(skin)
            .or_else(|| self.skin_materials.first())
            .and_then(|row| row.get(skin_ref))
            .map_or("", String::as_str)
    }
}

pub(crate) fn body_choices(parts: &[(i32, u32)], body: u32) -> Vec<u32> {
    parts
        .iter()
        .map(|&(base, count)| {
            let base = u32::try_from(base).unwrap_or(1).max(1);
            if count == 0 { 0 } else { (body / base) % count }
        })
        .collect()
}

pub(crate) fn body_key(parts: &[(i32, u32)], body: u32) -> u32 {
    body_choices(parts, body)
        .iter()
        .zip(parts)
        .map(|(c, &(base, _))| c.saturating_mul(u32::try_from(base).unwrap_or(1).max(1)))
        .fold(0u32, u32::saturating_add)
}

pub(crate) fn info_body_parts(info: &ModelInfo) -> Vec<(i32, u32)> {
    info.body_groups.iter().map(|b| (b.base, u32::try_from(b.choices.len()).unwrap_or(u32::MAX))).collect()
}

pub(crate) const SIDE_BOTH: u8 = 0;
pub(crate) const SIDE_FRONT: u8 = 1;
pub(crate) const SIDE_BACK: u8 = 2;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PickPart {
    pub body_part: usize,
    pub choice: usize,
    pub positions: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub sides: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PickGeometry {
    pub parts: Vec<PickPart>,
    pub body_parts: Vec<(i32, u32)>,
}

impl PickGeometry {
    pub(crate) fn of(model: &PreparedModel) -> Self {
        let parts = model
            .parts
            .iter()
            .chain(&model.variant_parts)
            .map(|p| {
                let sides = p
                    .indices
                    .chunks_exact(3)
                    .map(|t| {
                        let v = |i: u32| p.vertices.get(i as usize);
                        let (Some(a), Some(b), Some(c)) = (v(t[0]), v(t[1]), v(t[2])) else { return SIDE_BOTH };
                        let n = |x: &hp_render::Vertex| glam::Vec3::from_array(x.normal);
                        let shading = n(a) + n(b) + n(c);
                        if shading.length_squared() <= 1e-12 {
                            return SIDE_BOTH;
                        }
                        let (pa, pb, pc) = (
                            glam::Vec3::from_array(a.pos),
                            glam::Vec3::from_array(b.pos),
                            glam::Vec3::from_array(c.pos),
                        );
                        if shading.dot((pb - pa).cross(pc - pa)) < 0.0 { SIDE_BACK } else { SIDE_FRONT }
                    })
                    .collect();
                PickPart {
                    body_part: p.body_part,
                    choice: p.choice,
                    positions: p.vertices.iter().map(|v| v.pos).collect(),
                    indices: p.indices.clone(),
                    sides,
                }
            })
            .collect();
        Self { parts, body_parts: model.body_parts.clone() }
    }

    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.parts.iter().map(|p| p.positions.capacity() * 12 + p.indices.capacity() * 4 + p.sides.capacity()).sum()
    }

    pub(crate) fn parts_for(&self, body: Option<u32>) -> impl Iterator<Item = &PickPart> {
        let choices = body.map(|b| body_choices(&self.body_parts, b));
        self.parts.iter().filter(move |p| match &choices {
            Some(c) => c.get(p.body_part).copied().unwrap_or(0) as usize == p.choice,
            None => p.choice == 0,
        })
    }
}

pub(crate) const ANIM_SEPARATOR: char = '|';

pub(crate) fn model_data_key(path: &str, anim: Option<&str>) -> String {
    match anim.map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) => format!("{path}{ANIM_SEPARATOR}{}", a.to_ascii_lowercase()),
        None => path.to_owned(),
    }
}

pub(crate) fn model_job(fs: &hp_fs::GameFs, key: &str) -> Result<PreparedModel, String> {
    let (path, anim) = match key.split_once(ANIM_SEPARATOR) {
        Some((p, a)) => (p, Some(a)),
        None => (key, None),
    };
    let particle = anim == Some(crate::particle_model_draw::ANIMATION_KEY);
    let opts = hp_mdl::LoadOptions {
        body_groups: hp_mdl::BodyGroupSelect::All,
        tangents: true,
        bone_weights: particle,
        pose: if particle { hp_mdl::Pose::Bind } else { Default::default() },
        ..Default::default()
    };
    let files = hp_mdl::read_files(path, |p| fs.read(p)).map_err(|e| e.to_string())?;
    let mut model = files.load(&opts).map_err(|e| e.to_string())?;
    if !particle
        && let Some(anim) = anim
        && let Some(index) = model.sequences.iter().position(|s| s.name.eq_ignore_ascii_case(anim))
        && model.posed_sequence != Some(index)
    {
        let posed = hp_mdl::LoadOptions { pose: hp_mdl::Pose::Sequence(index), ..opts };
        if let Ok(m) = files.load(&posed) {
            model = m;
        }
    }
    let skin_count = model.skin_count();
    let sequence_frames = sequence_frames(&files.mdl, model.sequences.len().min(MAX_LISTED));
    let hp_mdl::ModelFiles { mdl, .. } = files;
    let particle_animation = particle.then(|| {
        Arc::new(crate::particle_model_draw::AnimationSource {
            ani: hp_mdl::animation_block_path(&mdl).and_then(|path| fs.read(&path)).map(Arc::new),
            mdl: Arc::new(mdl),
            bones: model.bones.clone(),
            meshes: model.meshes.clone(),
            sequences: model.sequences.clone(),
        })
    });
    let used_refs: Vec<usize> = {
        let mut seen = AHashSet::new();
        model.meshes.iter().map(|m| m.skin_ref).filter(|r| seen.insert(*r)).collect()
    };
    let max_ref = used_refs.iter().copied().max().unwrap_or(0);
    let mut exists: AHashMap<String, bool> = AHashMap::new();
    let mut skin_materials = Vec::with_capacity(skin_count);
    for skin in 0..skin_count {
        let mut row = vec![String::new(); max_ref + 1];
        for &skin_ref in &used_refs {
            let candidates = model.material_candidates(skin_ref, skin);
            let chosen = candidates
                .iter()
                .find(|c| *exists.entry((*c).clone()).or_insert_with(|| fs.exists(&format!("materials/{c}.vmt"))))
                .or_else(|| candidates.first())
                .cloned()
                .unwrap_or_default();
            if let Some(slot) = row.get_mut(skin_ref) {
                *slot = chosen;
            }
        }
        skin_materials.push(row);
    }
    let (parts, variant_parts): (Vec<PreparedPart>, Vec<PreparedPart>) = model
        .meshes
        .iter()
        .filter(|m| !m.indices.is_empty())
        .map(|m| PreparedPart {
            skin_ref: m.skin_ref,
            body_part: m.body_part,
            choice: m.model_index,
            vertices: m
                .vertices
                .iter()
                .map(|v| Vertex { pos: v.pos, normal: v.normal, uv: v.uv, alpha: 1.0 })
                .collect(),
            indices: m.indices.clone(),
            tangents: if m.tangents.len() == m.vertices.len() { m.tangents.clone() } else { Vec::new() },
            color_index: m.color_index.clone(),
        })
        .partition(|p| p.choice == 0);
    let body_parts: Vec<(i32, u32)> =
        model.body_parts.iter().map(|b| (b.base, u32::try_from(b.models.len()).unwrap_or(u32::MAX))).collect();
    let bounds = part_bounds(&parts).unwrap_or(model.bounds);
    let mut materials: Vec<String> = Vec::new();
    for part in &parts {
        if let Some(name) = skin_materials.first().and_then(|row| row.get(part.skin_ref))
            && !name.is_empty()
            && !materials.contains(name)
        {
            materials.push(name.clone());
        }
    }
    let (bmin, bmax) = model.bounds;
    let info = ModelInfo {
        path: path.to_owned(),
        bounds_min: DVec3::from(bmin.map(f64::from)),
        bounds_max: DVec3::from(bmax.map(f64::from)),
        skins: u32::try_from(skin_count).unwrap_or(u32::MAX),
        body_parts: u32::try_from(model.body_parts.len()).unwrap_or(u32::MAX),
        triangles: count(parts.iter().map(|p| p.indices.len() / 3).sum()),
        materials,
        is_static_prop: model.is_static_prop,
        surface_prop: model.surface_prop.clone(),
        hull_min: valid_hull(model.hull_min, model.hull_max).map(|(min, _)| min),
        hull_max: valid_hull(model.hull_min, model.hull_max).map(|(_, max)| max),
        body_groups: body_groups(&model),
        sequences: model.sequences.iter().take(MAX_LISTED).map(|s| s.name.clone()).collect(),
        sequence_frames,
        stats: hp_app::state::ModelStats {
            vertices: count(parts.iter().map(|p| p.vertices.len()).sum()),
            meshes: count(parts.len()),
            triangles_all: count(model.triangle_count()),
            vertices_all: count(model.vertex_count()),
            bones: count(model.bones.len()),
            lods: model.num_lods,
            mass: model.mass,
            version: model.version,
            hitboxes: count(model.hitboxes.len()),
        },
    };
    let helpers = preview_helpers(&model);
    let lod_switch = model.lod_switch_points.clone();
    let bind_bones = model
        .bones
        .iter()
        .map(|bone| {
            let m = bone_to_model(bone);
            let cols = m.matrix3.to_cols_array_2d();
            let t = m.translation.to_array();
            std::array::from_fn(|row| [cols[0][row], cols[1][row], cols[2][row], t[row]])
        })
        .collect();
    Ok(PreparedModel {
        info: Arc::new(info),
        parts,
        variant_parts,
        body_parts,
        skin_materials,
        bounds,
        helpers,
        lod_switch,
        particle_sets: model.hitbox_sets,
        particle_bones: model.bone_matrices,
        bind_bones,
        particle_animation,
    })
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PreviewHelpers {
    pub hitbox_edges: Vec<[f32; 3]>,
    pub bone_lines: Vec<[f32; 3]>,
    pub joints: Vec<[f32; 3]>,
    pub attachments: Vec<[f32; 3]>,
    pub illum: [f32; 3],
    pub seq_boxes: Vec<([f32; 3], [f32; 3])>,
}

fn bone_to_model(bone: &hp_mdl::Bone) -> glam::Affine3A {
    let m = bone.pose_to_bone;
    let rows = glam::Mat3::from_cols(
        glam::Vec3::new(m[0][0], m[1][0], m[2][0]),
        glam::Vec3::new(m[0][1], m[1][1], m[2][1]),
        glam::Vec3::new(m[0][2], m[1][2], m[2][2]),
    );
    let t = glam::Vec3::new(m[0][3], m[1][3], m[2][3]);
    let inv = rows.transpose();
    glam::Affine3A::from_mat3_translation(inv, -(inv * t))
}

fn preview_helpers(model: &hp_mdl::Model) -> PreviewHelpers {
    let finite = |p: glam::Vec3| p.is_finite().then_some(p.to_array());
    let to_model: Vec<glam::Affine3A> = model.bones.iter().map(bone_to_model).collect();
    let mut h = PreviewHelpers { illum: model.illum_position, ..PreviewHelpers::default() };
    for hb in &model.hitboxes {
        let Some(m) = usize::try_from(hb.bone).ok().and_then(|b| to_model.get(b)) else { continue };
        let (lo, hi) = (glam::Vec3::from(hb.min), glam::Vec3::from(hb.max));
        if !(lo.is_finite() && hi.is_finite()) {
            continue;
        }
        for (a, b) in box_edges(lo.to_array(), hi.to_array()) {
            h.hitbox_edges.push(m.transform_point3(glam::Vec3::from(a)).to_array());
            h.hitbox_edges.push(m.transform_point3(glam::Vec3::from(b)).to_array());
        }
    }
    for (i, bone) in model.bones.iter().enumerate() {
        let p = to_model[i].translation.into();
        let Some(p) = finite(p) else { continue };
        h.joints.push(p);
        if let Some(parent) = usize::try_from(bone.parent).ok().and_then(|b| to_model.get(b))
            && let Some(q) = finite(parent.translation.into())
        {
            h.bone_lines.push(q);
            h.bone_lines.push(p);
        }
    }
    for a in &model.attachments {
        let Some(m) = usize::try_from(a.bone).ok().and_then(|b| to_model.get(b)) else { continue };
        let local = glam::Vec3::new(a.local[0][3], a.local[1][3], a.local[2][3]);
        if let Some(p) = finite(m.transform_point3(local)) {
            h.attachments.push(p);
        }
    }
    h.seq_boxes = model.sequences.iter().map(|s| (s.bbox_min, s.bbox_max)).collect();
    h
}

pub(crate) fn box_edges(lo: [f32; 3], hi: [f32; 3]) -> impl Iterator<Item = ([f32; 3], [f32; 3])> {
    let corner = move |i: usize| {
        [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ]
    };
    (0..8usize).flat_map(move |a| {
        [1, 2, 4].into_iter().filter(move |bit| a & bit == 0).map(move |bit| (corner(a), corner(a | bit)))
    })
}

fn sequence_frames(mdl: &[u8], n: usize) -> Vec<u32> {
    (0..n).map(|i| hp_mdl::animation_clip(mdl, i).map_or(1, |c| count(c.frames).max(1))).collect()
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn part_bounds(parts: &[PreparedPart]) -> Option<([f32; 3], [f32; 3])> {
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for v in parts.iter().flat_map(|p| &p.vertices) {
        if v.pos.iter().all(|c| c.is_finite()) {
            for i in 0..3 {
                lo[i] = lo[i].min(v.pos[i]);
                hi[i] = hi[i].max(v.pos[i]);
            }
        }
    }
    (lo[0] <= hi[0]).then_some((lo, hi))
}

const MAX_LISTED: usize = 1024;

fn body_groups(model: &hp_mdl::Model) -> Vec<hp_app::state::BodyGroupInfo> {
    model
        .body_parts
        .iter()
        .take(MAX_LISTED)
        .map(|part| hp_app::state::BodyGroupInfo {
            name: part.name.clone(),
            base: part.base,
            choices: part.models.iter().take(MAX_LISTED).map(|m| m.name.clone()).collect(),
        })
        .collect()
}

fn valid_hull(min: [f32; 3], max: [f32; 3]) -> Option<(DVec3, DVec3)> {
    let min = DVec3::from(min.map(f64::from));
    let max = DVec3::from(max.map(f64::from));
    (min.is_finite() && max.is_finite() && min.cmplt(max).all()).then_some((min, max))
}

pub(crate) enum ModelEntry {
    Loading,
    Ready(Arc<PreparedModel>),
    Cached(Arc<ModelInfo>),
    Failed(String),
}

pub(crate) struct RenderModel {
    pub id: Option<ModelId>,
    pub materials: Vec<hp_render::MaterialId>,
    pub epoch: u64,
}

pub(crate) type VariantKey = (String, u32, u32);

#[derive(Default)]
pub(crate) struct Models {
    pub map: AHashMap<String, ModelEntry>,
    pub render: AHashMap<(String, u32), RenderModel>,
    pub variants: AHashMap<VariantKey, (RenderModel, std::sync::Weak<PreparedModel>)>,
    pub newly_ready: AHashSet<String>,
    pub last_used: AHashMap<String, u64>,
    pub pick: AHashMap<String, Arc<PickGeometry>>,
    pub settled: Option<(u64, std::time::Instant)>,
    pub trim_waiting: bool,
}

pub(crate) fn entity_matrix(origin: DVec3, angles: DVec3, scale: f64) -> DMat4 {
    let q: DQuat = hp_geom::angles_to_quat(angles);
    let s = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
    DMat4::from_scale_rotation_translation(DVec3::splat(s), q, origin)
}

pub(crate) fn transformed_bounds(min: DVec3, max: DVec3, m: &DMat4) -> hp_geom::Aabb {
    let mut out = hp_geom::Aabb::EMPTY;
    for i in 0..8 {
        let corner = DVec3::new(
            if i & 1 != 0 { max.x } else { min.x },
            if i & 2 != 0 { max.y } else { min.y },
            if i & 4 != 0 { max.z } else { min.z },
        );
        out.extend(m.transform_point3(corner));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms() {
        let m = entity_matrix(DVec3::new(100.0, 0.0, 0.0), DVec3::new(0.0, 90.0, 0.0), 1.0);
        let b = transformed_bounds(DVec3::new(0.0, -1.0, 0.0), DVec3::new(10.0, 1.0, 2.0), &m);
        assert!((b.min - DVec3::new(99.0, 0.0, 0.0)).length() < 1e-6, "{b:?}");
        assert!((b.max - DVec3::new(101.0, 10.0, 2.0)).length() < 1e-6, "{b:?}");
        let scaled = entity_matrix(DVec3::ZERO, DVec3::ZERO, 2.0);
        let b = transformed_bounds(DVec3::splat(-1.0), DVec3::splat(1.0), &scaled);
        assert_eq!(b.max, DVec3::splat(2.0));
        let bad = entity_matrix(DVec3::ZERO, DVec3::ZERO, f64::NAN);
        assert_eq!(bad, DMat4::IDENTITY);
    }

    #[test]
    fn part_materials() {
        let pm = PreparedModel {
            info: Arc::new(ModelInfo::default()),
            parts: Vec::new(),
            variant_parts: Vec::new(),
            body_parts: Vec::new(),
            skin_materials: vec![vec!["a".into(), "b".into()], vec!["c".into(), "d".into()]],
            bounds: ([0.0; 3], [0.0; 3]),
            helpers: PreviewHelpers::default(),
            lod_switch: Vec::new(),
            ..Default::default()
        };
        assert_eq!(pm.skin_count(), 2);
        assert_eq!(pm.part_material(1, 1), "d");
        assert_eq!(pm.part_material(9, 0), "a");
        assert_eq!(pm.part_material(0, 9), "");
    }

    #[test]
    fn body_groups_follow_the_engine_rule() {
        let part = |body_part: usize, choice: usize| PreparedPart {
            skin_ref: 0,
            body_part,
            choice,
            vertices: vec![Vertex { pos: [choice as f32, 0.0, 0.0], ..Default::default() }],
            indices: Vec::new(),
            tangents: Vec::new(),
            color_index: Vec::new(),
        };
        let pm = PreparedModel {
            info: Arc::new(ModelInfo::default()),
            parts: vec![part(0, 0), part(1, 0)],
            variant_parts: vec![part(0, 1), part(0, 2), part(1, 1)],
            body_parts: vec![(1, 3), (3, 2)],
            skin_materials: Vec::new(),
            bounds: ([0.0; 3], [0.0; 3]),
            helpers: PreviewHelpers::default(),
            lod_switch: Vec::new(),
            ..Default::default()
        };
        assert_eq!(pm.body_choices(0), vec![0, 0]);
        assert_eq!(pm.body_choices(5), vec![2, 1]);
        assert_eq!(pm.body_key(5), 5);
        assert_eq!(pm.body_key(6), 0, "6 wraps to the default look");
        let shown: Vec<(usize, usize)> = pm.parts_for_body(4).iter().map(|p| (p.body_part, p.choice)).collect();
        assert_eq!(shown, vec![(0, 1), (1, 1)]);
        assert_eq!(pm.parts_for_body(0).len(), 2);
        let bad = PreparedModel { body_parts: vec![(0, 0), (-5, 2)], ..pm };
        assert_eq!(bad.body_choices(u32::MAX).len(), 2);
        assert_eq!(model_data_key("models/a.mdl", Some(" Idle ")), "models/a.mdl|idle");
        assert_eq!(model_data_key("models/a.mdl", Some("")), "models/a.mdl");
        assert_eq!(part_bounds(&[part(0, 2)]), Some(([2.0, 0.0, 0.0], [2.0, 0.0, 0.0])));
    }
}

#[cfg(test)]
mod model_fuzz_tmp {
    #[test]
    #[ignore = "manual installed-model corpus diagnostic; machine-specific and memory intensive"]
    fn every_model_loads_without_panic() {
        let root = std::path::Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod");
        if !root.join("garrysmod").is_dir() {
            eprintln!("no Garry's Mod install: test skipped");
            return;
        }
        let fs = hp_fs::GameFs::for_gmod(root, &hp_fs::MountOptions::default()).expect("mount");
        let models = fs.models();
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let mut bad = Vec::new();
        for m in models.iter() {
            let name = m.name.clone();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| super::model_job(&fs, &name)));
            if r.is_err() {
                bad.push(name);
            }
        }
        std::panic::set_hook(prev);
        eprintln!("MODELFUZZ {} models, {} panics: {:?}", models.len(), bad.len(), &bad[..bad.len().min(30)]);
    }
}
