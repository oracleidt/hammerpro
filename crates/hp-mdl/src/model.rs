use crate::error::{FileKind, MdlError};
use crate::mdl::{
    self, BodyPart, Bone, FLAG_HAS_SHADOW_LOD, FLAG_OBSOLETE, FLAG_STATIC_PROP, Sequence, Studio, SubModel,
};
use crate::paths::compose_material;
use crate::pose::{Mat34, Pose, posing, skin_vertex};
use crate::reader::Result;
use crate::vtx::{self, Layout, StripGroup, Vtx};
use crate::vvd::{BoneWeight, VertexMap, Vvd};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelMesh {
    pub material_index: usize,
    pub skin_ref: usize,
    pub body_part: usize,
    pub model_index: usize,
    pub mesh_index: usize,
    pub vertices: Vec<ModelVertex>,
    pub tangents: Vec<[f32; 4]>,
    pub bone_weights: Vec<BoneWeight>,
    pub indices: Vec<u32>,
    pub color_index: Vec<u32>,
}

impl ModelMesh {
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum BodyGroupSelect {
    #[default]
    Default,
    All,
    Body(u32),
    Choices(Vec<u32>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LoadOptions {
    pub lod: u32,
    pub body_groups: BodyGroupSelect,
    pub pose: Pose,
    pub tangents: bool,
    pub bone_weights: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub name: String,
    pub version: i32,
    pub checksum: i32,
    pub flags: u32,
    pub is_static_prop: bool,
    pub eye_position: [f32; 3],
    pub illum_position: [f32; 3],
    pub hull_min: [f32; 3],
    pub hull_max: [f32; 3],
    pub view_bb_min: [f32; 3],
    pub view_bb_max: [f32; 3],
    pub bounds: ([f32; 3], [f32; 3]),
    pub mass: f32,
    pub contents: i32,
    pub surface_prop: String,
    pub keyvalues: String,
    pub materials: Vec<String>,
    pub material_dirs: Vec<String>,
    pub skins: Vec<Vec<u16>>,
    pub lod_texture_remap: Vec<usize>,
    pub meshes: Vec<ModelMesh>,
    pub bones: Vec<Bone>,
    pub bone_matrices: Vec<crate::SkinMatrix>,
    pub bone_names: Vec<String>,
    pub body_parts: Vec<BodyPart>,
    pub sequences: Vec<Sequence>,
    pub include_models: Vec<String>,
    pub hitboxes: Vec<crate::mdl::Hitbox>,
    pub hitbox_sets: Vec<crate::mdl::HitboxSet>,
    pub attachments: Vec<crate::mdl::Attachment>,
    pub lod: u32,
    pub num_lods: u32,
    pub lod_switch_points: Vec<f32>,
    pub posed_sequence: Option<usize>,
}

impl Model {
    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(ModelMesh::triangle_count).sum()
    }

    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(|m| m.vertices.len()).sum()
    }

    pub fn skin_count(&self) -> usize {
        self.skins.len().max(1)
    }

    pub fn texture_index(&self, skin_ref: usize, skin: usize) -> Option<usize> {
        let tex = match self.skins.get(skin).or_else(|| self.skins.first()) {
            Some(row) => usize::from(*row.get(skin_ref)?),
            None => skin_ref,
        };
        let tex = self.lod_texture_remap.get(tex).copied().unwrap_or(tex);
        (tex < self.materials.len()).then_some(tex)
    }

    pub fn material_candidates(&self, skin_ref: usize, skin: usize) -> Vec<String> {
        self.texture_index(skin_ref, skin).map(|t| self.texture_candidates(t)).unwrap_or_default()
    }

    pub fn texture_candidates(&self, texture_index: usize) -> Vec<String> {
        let Some(texture) = self.materials.get(texture_index) else {
            return Vec::new();
        };
        if self.flags & FLAG_OBSOLETE != 0 {
            return vec!["models/obsolete/obsolete".to_string()];
        }
        let mut out: Vec<String> = Vec::with_capacity(self.material_dirs.len().max(1));
        if self.material_dirs.is_empty() {
            out.push(compose_material("", texture));
        }
        for dir in &self.material_dirs {
            let name = compose_material(dir, texture);
            if !name.is_empty() && !out.contains(&name) {
                out.push(name);
            }
        }
        out
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModelLayout {
    pub start: usize,
    pub meshes: Vec<(usize, usize)>,
}

fn non_negative(v: i32, what: &'static str) -> Result<usize> {
    usize::try_from(v).map_err(|_| MdlError::InvalidValue { file: FileKind::Mdl, what, value: i64::from(v) })
}

pub(crate) fn stored_layout(studio: &Studio) -> Result<Vec<Vec<ModelLayout>>> {
    studio
        .body_parts
        .iter()
        .map(|bp| {
            bp.models
                .iter()
                .map(|model| {
                    let start = non_negative(model.vertex_index, "model vertex index")? / crate::vvd::VERTEX_SIZE;
                    let meshes = model
                        .meshes
                        .iter()
                        .map(|m| {
                            Ok((
                                non_negative(m.vertex_offset, "mesh vertex offset")?,
                                non_negative(m.num_vertices, "mesh vertex count")?,
                            ))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    Ok(ModelLayout { start, meshes })
                })
                .collect()
        })
        .collect()
}

fn selected_models(studio: &Studio, select: &BodyGroupSelect) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (bp_index, bp) in studio.body_parts.iter().enumerate() {
        let n = bp.models.len();
        if n == 0 {
            continue;
        }
        match select {
            BodyGroupSelect::All => out.extend((0..n).map(|m| (bp_index, m))),
            BodyGroupSelect::Default => out.push((bp_index, 0)),
            BodyGroupSelect::Body(body) => {
                let base = u32::try_from(bp.base).ok().filter(|&b| b > 0).unwrap_or(1);
                let choice = (body / base) as usize % n;
                out.push((bp_index, choice));
            }
            BodyGroupSelect::Choices(choices) => {
                let choice = choices.get(bp_index).map_or(0, |&c| c as usize);
                out.push((bp_index, if choice < n { choice } else { 0 }));
            }
        }
    }
    out
}

struct Builder<'a, 'd> {
    studio: &'a Studio,
    vvd: &'a Vvd<'d>,
    vtx: &'a Vtx<'d>,
    map: &'a VertexMap,
    layout: &'a [Vec<ModelLayout>],
    lod: usize,
    opts: &'a LoadOptions,
    skin: Option<&'a [Mat34]>,
    remap: Vec<u32>,
    group_map: Vec<u32>,
    color_bases: Vec<Vec<Vec<Vec<usize>>>>,
    budget: (usize, usize),
    emitted: (usize, usize),
}

impl Builder<'_, '_> {
    fn build(&mut self, selection: &[(usize, usize)], vtx_layout: Layout) -> Result<Vec<ModelMesh>> {
        self.emitted = (0, 0);
        self.color_bases = self.color_bases(vtx_layout).unwrap_or_default();
        let mut meshes = Vec::new();
        for &(bp, m) in selection {
            self.build_model(bp, m, vtx_layout, &mut meshes)?;
        }
        Ok(meshes)
    }

    fn color_bases(&self, vtx_layout: Layout) -> Result<Vec<Vec<Vec<Vec<usize>>>>> {
        let vtx = self.vtx;
        let mut at = 0usize;
        let mut out = Vec::with_capacity(vtx.num_body_parts);
        for bp in 0..vtx.num_body_parts {
            let (models_start, num_models) = vtx.body_part(bp)?;
            let mut models = Vec::with_capacity(num_models);
            for m in 0..num_models {
                let (lods_start, num_lods) = vtx.model_lods(Vtx::model_at(models_start, m))?;
                let mut lods = Vec::with_capacity(num_lods);
                for l in 0..num_lods {
                    let (meshes_start, num_meshes) = vtx.lod_meshes(Vtx::lod_at(lods_start, l))?;
                    let mut meshes = Vec::with_capacity(num_meshes);
                    for k in 0..num_meshes {
                        meshes.push(at);
                        let (groups_start, num_groups) =
                            vtx.mesh_strip_groups(Vtx::mesh_at(meshes_start, k), vtx_layout)?;
                        for g in 0..num_groups {
                            at = at.saturating_add(vtx.strip_group(groups_start, g, vtx_layout)?.num_verts);
                        }
                    }
                    lods.push(meshes);
                }
                models.push(lods);
            }
            out.push(models);
        }
        Ok(out)
    }

    fn spend(&mut self, vertices: usize, indices: usize) -> Result<()> {
        self.emitted.0 += vertices;
        self.emitted.1 += indices;
        if self.emitted.0 > self.budget.0 || self.emitted.1 > self.budget.1 {
            return Err(MdlError::InvalidValue {
                file: FileKind::Vtx,
                what: "output size (aliased strip data)",
                value: (self.emitted.0 + self.emitted.1) as i64,
            });
        }
        Ok(())
    }

    fn build_model(&mut self, bp: usize, m: usize, vtx_layout: Layout, out: &mut Vec<ModelMesh>) -> Result<()> {
        let vtx = self.vtx;
        let r = vtx.reader();
        let raw = self
            .studio
            .body_parts
            .get(bp)
            .and_then(|p| p.models.get(m))
            .ok_or_else(|| r.invalid("model index", m as i64))?;
        let layout = self.layout.get(bp).and_then(|p| p.get(m)).ok_or_else(|| r.invalid("model index", m as i64))?;

        let (models_start, num_models) = vtx.body_part(bp)?;
        let num_mdl_models = self.studio.body_parts.get(bp).map_or(0, |p| p.models.len());
        if num_models != num_mdl_models {
            return Err(mismatch("model count", num_mdl_models, num_models));
        }
        let (lods_start, num_lods) = vtx.model_lods(Vtx::model_at(models_start, m))?;
        if num_lods == 0 {
            return Ok(());
        }
        let lod = self.lod.min(num_lods - 1);
        let (meshes_start, num_meshes) = vtx.lod_meshes(Vtx::lod_at(lods_start, lod))?;
        if num_meshes != raw.meshes.len() {
            return Err(mismatch("mesh count", raw.meshes.len(), num_meshes));
        }

        for (mesh_index, (raw_mesh, &(offset, count))) in raw.meshes.iter().zip(&layout.meshes).enumerate() {
            let base = layout.start.saturating_add(offset);
            if base.saturating_add(count) > self.map.len() {
                return Err(MdlError::InvalidValue {
                    file: FileKind::Vvd,
                    what: "mesh vertex range",
                    value: base.saturating_add(count) as i64,
                });
            }
            let skin_ref = non_negative(raw_mesh.material, "mesh material")?;
            let mut mesh = ModelMesh { skin_ref, body_part: bp, model_index: m, mesh_index, ..ModelMesh::default() };
            self.remap.clear();
            self.remap.resize(count, u32::MAX);

            let vtx_mesh = Vtx::mesh_at(meshes_start, mesh_index);
            let (groups_start, num_groups) = vtx.mesh_strip_groups(vtx_mesh, vtx_layout)?;
            let (mut verts, mut indices) = (0usize, 0usize);
            for g in 0..num_groups {
                let group = vtx.strip_group(groups_start, g, vtx_layout)?;
                verts = verts.saturating_add(group.num_verts);
                indices = indices.saturating_add(group.num_indices);
            }
            let verts = verts.min(count);
            mesh.vertices.reserve_exact(verts);
            mesh.indices.reserve_exact(indices.min(self.budget.1));
            if self.opts.tangents {
                mesh.tangents.reserve_exact(verts);
            }
            if self.opts.bone_weights {
                mesh.bone_weights.reserve_exact(verts);
            }
            let mut color = self
                .color_bases
                .get(bp)
                .and_then(|p| p.get(m))
                .and_then(|p| p.get(lod))
                .and_then(|p| p.get(mesh_index))
                .copied();
            for g in 0..num_groups {
                let group = vtx.strip_group(groups_start, g, vtx_layout)?;
                let before = (mesh.vertices.len(), mesh.indices.len());
                self.map_group_vertices(&group, base, color, &mut mesh)?;
                color = color.map(|c| c.saturating_add(group.num_verts));
                self.emit_group_triangles(&group, vtx_layout, &mut mesh)?;
                self.spend(mesh.vertices.len() - before.0, mesh.indices.len() - before.1)?;
            }
            if mesh.color_index.len() != mesh.vertices.len() {
                mesh.color_index.clear();
            }
            if !mesh.indices.is_empty() {
                out.push(mesh);
            }
        }
        Ok(())
    }

    fn map_group_vertices(
        &mut self,
        group: &StripGroup,
        base: usize,
        color: Option<usize>,
        mesh: &mut ModelMesh,
    ) -> Result<()> {
        let r = self.vtx.reader();
        let bytes = self.vtx.vertex_bytes(group)?;
        self.group_map.clear();
        self.group_map.reserve(group.num_verts);
        for (k, v) in bytes.chunks_exact(vtx::VERTEX_SIZE).enumerate() {
            let orig = match v.get(4..6) {
                Some(&[lo, hi]) => usize::from(u16::from_le_bytes([lo, hi])),
                _ => return Err(r.invalid("strip group vertex", 0)),
            };
            let slot = self.remap.get_mut(orig).ok_or_else(|| r.invalid("origMeshVertID", orig as i64))?;
            if *slot == u32::MAX {
                let src = self.map.get(base + orig).ok_or_else(|| r.invalid("origMeshVertID", (base + orig) as i64))?;
                let (mut pos, mut normal, uv) = self.vvd.vertex(src)?;
                let mut tangent = if self.opts.tangents { self.vvd.tangent(src)? } else { None };
                if self.skin.is_some() || self.opts.bone_weights {
                    let weight = self.vvd.bone_weight(src)?;
                    if let Some(skin) = self.skin {
                        (pos, normal, tangent) = skin_vertex(skin, &weight, pos, normal, tangent);
                    }
                    if self.opts.bone_weights {
                        mesh.bone_weights.push(weight);
                    }
                }
                *slot = mesh.vertices.len() as u32;
                mesh.vertices.push(ModelVertex { pos, normal, uv });
                if let Some(c) = color.and_then(|c| u32::try_from(c.saturating_add(k)).ok()) {
                    mesh.color_index.push(c);
                }
                if let Some(t) = tangent {
                    mesh.tangents.push(t);
                }
            }
            self.group_map.push(*slot);
        }
        Ok(())
    }

    fn emit_group_triangles(&self, group: &StripGroup, vtx_layout: Layout, mesh: &mut ModelMesh) -> Result<()> {
        let vtx = self.vtx;
        let r = vtx.reader();
        let index_bytes = vtx.index_bytes(group)?;
        let group_map = &self.group_map;
        let index = |i: usize| -> Result<u32> {
            let raw = match index_bytes.get(i * 2..i * 2 + 2) {
                Some(&[lo, hi]) => usize::from(u16::from_le_bytes([lo, hi])),
                _ => return Err(r.invalid("strip index", i as i64)),
            };
            group_map.get(raw).copied().ok_or_else(|| r.invalid("strip group vertex index", raw as i64))
        };

        for s in 0..group.num_strips.max(1) {
            let strip = if group.num_strips == 0 {
                vtx::Strip { index_offset: 0, num_indices: group.num_indices, flags: vtx::STRIP_IS_TRILIST }
            } else {
                vtx.strip(group, s, vtx_layout)?
            };
            let is_tristrip = !vtx_layout.extended
                && strip.flags & vtx::STRIP_IS_TRISTRIP != 0
                && strip.flags & vtx::STRIP_IS_TRILIST == 0;
            let start = strip.index_offset;
            if is_tristrip {
                for k in 0..strip.num_indices.saturating_sub(2) {
                    let (a, b, c) = (index(start + k)?, index(start + k + 1)?, index(start + k + 2)?);
                    if k % 2 == 0 { push_triangle(mesh, a, b, c) } else { push_triangle(mesh, a, c, b) }
                }
            } else {
                if strip.num_indices % 3 != 0 {
                    return Err(r.invalid("triangle list index count", strip.num_indices as i64));
                }
                for t in 0..strip.num_indices / 3 {
                    let i = start + t * 3;
                    push_triangle(mesh, index(i)?, index(i + 1)?, index(i + 2)?);
                }
            }
        }
        Ok(())
    }
}

#[inline]
fn push_triangle(mesh: &mut ModelMesh, a: u32, b: u32, c: u32) {
    if a != b && b != c && a != c {
        mesh.indices.extend_from_slice(&[a, c, b]);
    }
}

fn mismatch(what: &'static str, mdl: usize, other: usize) -> MdlError {
    MdlError::StructureMismatch { file: FileKind::Vtx, what, mdl: mdl as i64, other: other as i64 }
}

fn vertex_bounds(meshes: &[ModelMesh]) -> Option<([f32; 3], [f32; 3])> {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    let mut any = false;
    for v in meshes.iter().flat_map(|m| m.vertices.iter()) {
        if v.pos.iter().all(|c| c.is_finite()) {
            any = true;
            for i in 0..3 {
                min[i] = min[i].min(v.pos[i]);
                max[i] = max[i].max(v.pos[i]);
            }
        }
    }
    any.then_some((min, max))
}

fn select_lod(studio: &Studio, vtx: &Vtx<'_>, requested: u32) -> (usize, usize) {
    let mut lods = vtx.num_lods.max(1);
    if studio.info.flags & FLAG_HAS_SHADOW_LOD != 0 && lods > 1 {
        lods -= 1;
    }
    ((requested as usize).min(lods - 1), lods)
}

pub fn load(mdl: &[u8], vvd: &[u8], vtx: &[u8], opts: &LoadOptions) -> std::result::Result<Model, MdlError> {
    load_with_layout(mdl, vvd, vtx, opts, None)
}

pub(crate) fn load_with_layout(
    mdl: &[u8],
    vvd: &[u8],
    vtx: &[u8],
    opts: &LoadOptions,
    forced: Option<Layout>,
) -> Result<Model> {
    let vtx_bytes = vtx;
    let studio = mdl::parse(mdl)?;
    let vvd = Vvd::parse(vvd)?;
    let vtx = Vtx::parse(vtx)?;
    let checksum = studio.info.checksum;
    if vvd.checksum != checksum {
        return Err(MdlError::ChecksumMismatch {
            file: FileKind::Vvd,
            expected: checksum as u32,
            found: vvd.checksum as u32,
        });
    }
    if vtx.checksum != checksum {
        return Err(MdlError::ChecksumMismatch {
            file: FileKind::Vtx,
            expected: checksum as u32,
            found: vtx.checksum as u32,
        });
    }
    if vtx.num_body_parts != studio.body_parts.len() {
        return Err(mismatch("body part count", studio.body_parts.len(), vtx.num_body_parts));
    }

    let (lod, num_lods) = select_lod(&studio, &vtx, opts.lod);
    let map = vvd.vertex_map(0)?;
    let layout = stored_layout(&studio)?;
    let selection = selected_models(&studio, &opts.body_groups);
    let posed = posing(mdl, &studio, opts.pose);

    let mut builder = Builder {
        studio: &studio,
        vvd: &vvd,
        vtx: &vtx,
        map: &map,
        layout: &layout,
        lod,
        opts,
        skin: posed.skin.as_deref(),
        remap: Vec::new(),
        group_map: Vec::new(),
        color_bases: Vec::new(),
        budget: (vtx_bytes.len() / vtx::VERTEX_SIZE + 1024, vtx_bytes.len().saturating_mul(2).saturating_add(1024)),
        emitted: (0, 0),
    };
    let (first, second) = if studio.info.version >= 49 {
        (Layout::EXTENDED, Layout::STANDARD)
    } else {
        (Layout::STANDARD, Layout::EXTENDED)
    };
    let meshes = match forced {
        Some(layout) => builder.build(&selection, layout)?,
        None => {
            let mut order = [first, second];
            if !vtx.strips_partition(first) && vtx.strips_partition(second) {
                order.swap(0, 1);
            }
            match builder.build(&selection, order[0]) {
                Ok(meshes) => meshes,
                Err(err) => builder.build(&selection, order[1]).map_err(|_| err)?,
            }
        }
    };

    let mut materials = studio.textures;
    let mut lod_texture_remap: Vec<usize> = (0..materials.len()).collect();
    for (texture, name) in vtx.material_replacements(lod).unwrap_or_default() {
        if let Some(slot) = lod_texture_remap.get_mut(texture) {
            *slot = match materials.iter().position(|m| m.eq_ignore_ascii_case(&name)) {
                Some(existing) => existing,
                None => {
                    materials.push(name);
                    materials.len() - 1
                }
            };
        }
    }

    let info = studio.info;
    let bounds = vertex_bounds(&meshes)
        .unwrap_or_else(|| mdl::header_bounds(info.hull_min, info.hull_max, info.view_bb_min, info.view_bb_max));
    let body_parts = studio
        .body_parts
        .iter()
        .map(|bp| BodyPart {
            name: bp.name.clone(),
            base: bp.base,
            models: bp
                .models
                .iter()
                .map(|m| SubModel {
                    name: m.name.clone(),
                    num_meshes: m.meshes.len() as u32,
                    num_vertices: u32::try_from(m.num_vertices).unwrap_or(0),
                })
                .collect(),
        })
        .collect();

    let mut model = Model {
        name: info.name,
        version: info.version,
        checksum,
        flags: info.flags,
        is_static_prop: info.flags & FLAG_STATIC_PROP != 0,
        eye_position: info.eye_position,
        illum_position: info.illum_position,
        hull_min: info.hull_min,
        hull_max: info.hull_max,
        view_bb_min: info.view_bb_min,
        view_bb_max: info.view_bb_max,
        bounds,
        mass: info.mass,
        contents: info.contents,
        surface_prop: info.surface_prop,
        keyvalues: info.keyvalues,
        materials,
        material_dirs: studio.cd_textures,
        skins: studio.skins,
        lod_texture_remap,
        meshes,
        bone_names: studio.bones.iter().map(|b| b.name.clone()).collect(),
        bones: studio.bones,
        bone_matrices: posed.bones,
        body_parts,
        sequences: studio.sequences,
        include_models: studio.include_models,
        hitboxes: studio.hitboxes,
        hitbox_sets: studio.hitbox_sets,
        attachments: studio.attachments,
        lod: lod as u32,
        num_lods: num_lods as u32,
        lod_switch_points: vtx.lod_switch_points(num_lods),
        posed_sequence: posed.sequence,
    };
    let skin0: Vec<usize> = model.meshes.iter().map(|m| model.texture_index(m.skin_ref, 0).unwrap_or(0)).collect();
    for (mesh, texture) in model.meshes.iter_mut().zip(skin0) {
        mesh.material_index = texture;
    }
    Ok(model)
}
