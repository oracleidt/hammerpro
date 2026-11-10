mod anim;
mod deps;
mod error;
mod mdl;
mod model;
mod paths;
mod phy;
mod pose;
mod reader;
mod vtx;
mod vvd;

pub use anim::{AnimRef, AnimSet};
pub use deps::{MdlDependencies, gib_models, read_dependencies};
pub use error::{FileKind, MdlError};
pub use mdl::{
    Attachment, BodyPart, Bone, FLAG_HAS_SHADOW_LOD, FLAG_OBSOLETE, FLAG_STATIC_PROP, Hitbox, HitboxSet, MAX_VERSION,
    MIN_VERSION, MdlHeaderInfo, Sequence, SubModel, select_weighted_activity,
};
pub use model::{BodyGroupSelect, LoadOptions, Model, ModelMesh, ModelVertex, load};
pub use paths::{normalize_material_name, sibling_paths};
pub use phy::{CollisionModel, CollisionSolid, parse_phy, parse_phy_for_model};
pub use pose::{
    AnimClip, Pose, SkinMatrix, animation_block_path, animation_clip, pose_vertex, skin_matrices, skin_matrices_with,
};

pub fn pose_vertex_with_tangent(
    matrices: &[SkinMatrix],
    weight: &BoneWeight,
    pos: [f32; 3],
    normal: [f32; 3],
    tangent: Option<[f32; 4]>,
) -> ([f32; 3], [f32; 3], Option<[f32; 4]>) {
    pose::skin_vertex(matrices, weight, pos, normal, tangent)
}
pub use vvd::BoneWeight;

pub fn load_from(
    mdl_path: &str,
    read: impl FnMut(&str) -> Option<Vec<u8>>,
    opts: &LoadOptions,
) -> Result<Model, MdlError> {
    read_files(mdl_path, read)?.load(opts)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModelFiles {
    pub mdl: Vec<u8>,
    pub vvd: Vec<u8>,
    pub vtx: Vec<u8>,
}

impl ModelFiles {
    pub fn load(&self, opts: &LoadOptions) -> Result<Model, MdlError> {
        load(&self.mdl, &self.vvd, &self.vtx, opts)
    }
}

pub fn read_files(mdl_path: &str, mut read: impl FnMut(&str) -> Option<Vec<u8>>) -> Result<ModelFiles, MdlError> {
    let missing = |file, path: &str| MdlError::MissingFile { file, path: path.to_string() };
    let (vvd_path, vtx_paths) = sibling_paths(mdl_path);
    let mdl = read(mdl_path).ok_or_else(|| missing(FileKind::Mdl, mdl_path))?;
    let vvd = read(&vvd_path).ok_or_else(|| missing(FileKind::Vvd, &vvd_path))?;
    let vtx = vtx_paths
        .iter()
        .find_map(|p| read(p))
        .ok_or_else(|| missing(FileKind::Vtx, vtx_paths.last().map_or("", String::as_str)))?;
    Ok(ModelFiles { mdl, vvd, vtx })
}

pub fn raw_mesh_positions(mdl: &[u8], vvd: &[u8]) -> Result<Vec<Vec<[f32; 3]>>, MdlError> {
    let studio = mdl::parse(mdl)?;
    let vvd = vvd::Vvd::parse(vvd)?;
    let mut out = Vec::new();
    for part in &studio.body_parts {
        for model in &part.models {
            let base = usize::try_from(model.vertex_index).unwrap_or(0) / vvd::VERTEX_SIZE;
            for mesh in &model.meshes {
                let first = base + usize::try_from(mesh.vertex_offset).unwrap_or(0);
                let count = usize::try_from(mesh.num_vertices).unwrap_or(0);
                let points =
                    (first..first + count).map(|i| vvd.vertex(i).map(|v| v.0)).collect::<Result<Vec<_>, _>>()?;
                out.push(points);
            }
        }
    }
    Ok(out)
}

pub fn phy_path(mdl_path: &str) -> String {
    let stem = mdl_path.strip_suffix(".mdl").or_else(|| mdl_path.strip_suffix(".MDL")).unwrap_or(mdl_path);
    format!("{stem}.phy")
}

pub fn read_header(mdl: &[u8]) -> Result<MdlHeaderInfo, MdlError> {
    mdl::parse_header(mdl)
}

#[cfg(test)]
extern crate self as hp_mdl;

#[cfg(test)]
mod corpus_tests;
#[cfg(test)]
mod synthetic_tests;
