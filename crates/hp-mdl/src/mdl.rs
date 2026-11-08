use crate::error::{FileKind, MdlError};
use crate::reader::{Reader, Result};

pub const FLAG_STATIC_PROP: u32 = 1 << 4;
pub const FLAG_HAS_SHADOW_LOD: u32 = 1 << 6;
pub const FLAG_OBSOLETE: u32 = 1 << 9;

pub const MIN_VERSION: i32 = 44;
pub const MAX_VERSION: i32 = 49;

const MAGIC: &[u8; 4] = b"IDST";
const HEADER_SIZE: usize = 408;

const BONE_SIZE: usize = 216;
const TEXTURE_SIZE: usize = 64;
const BODYPART_SIZE: usize = 16;
const MODEL_SIZE: usize = 148;
const MESH_SIZE: usize = 116;
const MODELGROUP_SIZE: usize = 8;
pub(crate) const SEQDESC_SIZE: usize = 212;

const H_VERSION: usize = 4;
const H_CHECKSUM: usize = 8;
const H_NAME: usize = 12;
const H_LENGTH: usize = 76;
const H_EYE: usize = 80;
const H_ILLUM: usize = 92;
const H_HULL_MIN: usize = 104;
const H_HULL_MAX: usize = 116;
const H_VIEW_MIN: usize = 128;
const H_VIEW_MAX: usize = 140;
const H_FLAGS: usize = 152;
const H_NUM_BONES: usize = 156;
const H_NUM_HITBOX_SETS: usize = 172;
pub(crate) const H_NUM_LOCAL_ANIM: usize = 180;
pub(crate) const H_NUM_LOCAL_SEQ: usize = 188;
const H_NUM_TEXTURES: usize = 204;
const H_NUM_CD_TEXTURES: usize = 212;
const H_NUM_SKIN_REF: usize = 220;
const H_NUM_SKIN_FAMILIES: usize = 224;
const H_SKIN_INDEX: usize = 228;
const H_NUM_BODYPARTS: usize = 232;
const H_NUM_ATTACHMENTS: usize = 240;
const H_NUM_FLEX_CONTROLLERS: usize = 268;
const H_NUM_POSE_PARAMS: usize = 300;
const H_SURFACEPROP: usize = 308;
const H_KEYVALUE_INDEX: usize = 312;
const H_KEYVALUE_SIZE: usize = 316;
const H_MASS: usize = 328;
const H_CONTENTS: usize = 332;
const H_NUM_INCLUDE_MODELS: usize = 336;
const H_ROOT_LOD: usize = 377;
const H_NUM_ALLOWED_ROOT_LODS: usize = 378;
const H_STUDIOHDR2_INDEX: usize = 400;
const H2_NAME_INDEX: usize = 20;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MdlHeaderInfo {
    pub name: String,
    pub version: i32,
    pub checksum: i32,
    pub length: i32,
    pub flags: u32,
    pub eye_position: [f32; 3],
    pub illum_position: [f32; 3],
    pub hull_min: [f32; 3],
    pub hull_max: [f32; 3],
    pub view_bb_min: [f32; 3],
    pub view_bb_max: [f32; 3],
    pub mass: f32,
    pub contents: i32,
    pub surface_prop: String,
    pub keyvalues: String,
    pub root_lod: u8,
    pub num_allowed_root_lods: u8,
    pub num_bones: u32,
    pub num_textures: u32,
    pub num_cd_textures: u32,
    pub num_skin_refs: u32,
    pub num_skin_families: u32,
    pub num_body_parts: u32,
    pub num_sequences: u32,
    pub num_animations: u32,
    pub num_attachments: u32,
    pub num_include_models: u32,
    pub num_flex_controllers: u32,
    pub num_pose_parameters: u32,
}

impl MdlHeaderInfo {
    pub fn is_static_prop(&self) -> bool {
        self.flags & FLAG_STATIC_PROP != 0
    }

    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        header_bounds(self.hull_min, self.hull_max, self.view_bb_min, self.view_bb_max)
    }
}

pub(crate) fn header_bounds(
    hull_min: [f32; 3],
    hull_max: [f32; 3],
    view_min: [f32; 3],
    view_max: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    let valid = |min: [f32; 3], max: [f32; 3]| {
        min.iter().chain(max.iter()).all(|v| v.is_finite())
            && min.iter().zip(max.iter()).all(|(a, b)| a <= b)
            && min.iter().zip(max.iter()).any(|(a, b)| a < b)
    };
    if valid(hull_min, hull_max) {
        (hull_min, hull_max)
    } else if valid(view_min, view_max) {
        (view_min, view_max)
    } else {
        ([0.0; 3], [0.0; 3])
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attachment {
    pub name: String,
    pub bone: i32,
    pub local: [[f32; 4]; 3],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HitboxSet {
    pub name: String,
    pub hitboxes: Vec<Hitbox>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hitbox {
    pub bone: i32,
    pub group: i32,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bone {
    pub name: String,
    pub parent: i32,
    pub pos: [f32; 3],
    pub quat: [f32; 4],
    pub rot: [f32; 3],
    pub pos_scale: [f32; 3],
    pub rot_scale: [f32; 3],
    pub pose_to_bone: [[f32; 4]; 3],
    pub flags: u32,
    pub surface_prop: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyPart {
    pub name: String,
    pub base: i32,
    pub models: Vec<SubModel>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubModel {
    pub name: String,
    pub num_meshes: u32,
    pub num_vertices: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sequence {
    pub name: String,
    pub activity: String,
    pub activity_weight: i32,
    pub flags: u32,
    pub bbox_min: [f32; 3],
    pub bbox_max: [f32; 3],
}

pub fn select_weighted_activity(
    sequences: &[Sequence],
    activity: &str,
    draw: impl FnOnce(i32) -> i32,
) -> Option<usize> {
    if activity.is_empty() {
        return None;
    }
    let matching = || sequences.iter().enumerate().filter(|(_, s)| s.activity.eq_ignore_ascii_case(activity));
    let total: u64 = matching().map(|(_, s)| u64::from(s.activity_weight.unsigned_abs())).sum();
    if matching().next().is_none() || total > i32::MAX as u64 {
        return None;
    }
    let mut ticket = draw(total as i32).clamp(0, (total as i32 - 1).max(0)) as u32;
    let mut last = None;
    for (index, sequence) in matching() {
        last = Some(index);
        let weight = sequence.activity_weight.unsigned_abs().max(1);
        if ticket < weight {
            return Some(index);
        }
        ticket -= weight;
    }
    last
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RawMesh {
    pub material: i32,
    pub num_vertices: i32,
    pub vertex_offset: i32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RawModel {
    pub name: String,
    pub num_vertices: i32,
    pub vertex_index: i32,
    pub meshes: Vec<RawMesh>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RawBodyPart {
    pub name: String,
    pub base: i32,
    pub models: Vec<RawModel>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Studio {
    pub info: MdlHeaderInfo,
    pub bones: Vec<Bone>,
    pub textures: Vec<String>,
    pub cd_textures: Vec<String>,
    pub skins: Vec<Vec<u16>>,
    pub body_parts: Vec<RawBodyPart>,
    pub include_models: Vec<String>,
    pub sequences: Vec<Sequence>,
    pub hitboxes: Vec<Hitbox>,
    pub hitbox_sets: Vec<HitboxSet>,
    pub attachments: Vec<Attachment>,
}

const MAX_SKIN_FAMILIES: i32 = 1024;
const MAX_CD_TEXTURES: i32 = 256;
const MAX_INCLUDE_MODELS: i32 = 1024;

fn capped(r: &Reader<'_>, value: i32, max: i32, what: &'static str) -> Result<i32> {
    if value > max {
        return Err(r.invalid(what, i64::from(value)));
    }
    Ok(value)
}

fn count(r: &Reader<'_>, offset: usize, what: &'static str) -> Result<u32> {
    let v = r.i32(offset)?;
    u32::try_from(v).map_err(|_| r.invalid(what, i64::from(v)))
}

fn rel_str(r: &Reader<'_>, base: usize, rel: i32, what: &'static str) -> Result<String> {
    if rel == 0 {
        return Ok(String::new());
    }
    let at = r.rel(base, rel, what)?;
    r.cstr(at, what)
}

pub(crate) fn parse_header(data: &[u8]) -> Result<MdlHeaderInfo> {
    let r = Reader::new(data, FileKind::Mdl);
    let magic = r.bytes(0, 4, "header")?;
    if magic != MAGIC {
        return Err(MdlError::BadMagic {
            file: FileKind::Mdl,
            expected: "IDST",
            found: String::from_utf8_lossy(magic).into_owned(),
        });
    }
    let version = r.i32(H_VERSION)?;
    if !(MIN_VERSION..=MAX_VERSION).contains(&version) {
        return Err(MdlError::UnsupportedVersion { file: FileKind::Mdl, version });
    }
    r.check(0, HEADER_SIZE, "studiohdr_t")?;

    let mut name = r.fixed_str(H_NAME, 64, "name")?;
    let hdr2 = r.i32(H_STUDIOHDR2_INDEX)?;
    if hdr2 > 0
        && let Ok(base) = r.rel(0, hdr2, "studiohdr2_t")
        && let Ok(rel) = r.i32(base + H2_NAME_INDEX)
        && rel != 0
        && let Ok(long) = rel_str(&r, base, rel, "long name")
        && !long.is_empty()
    {
        name = long;
    }

    let surface_prop = rel_str(&r, 0, r.i32(H_SURFACEPROP)?, "surface prop")?;
    let kv_size = r.i32(H_KEYVALUE_SIZE)?;
    let keyvalues = if kv_size > 0 {
        let start = r.rel(0, r.i32(H_KEYVALUE_INDEX)?, "keyvalues")?;
        let bytes = r.bytes(start, kv_size as usize, "keyvalues")?;
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).into_owned()
    } else {
        String::new()
    };

    Ok(MdlHeaderInfo {
        name,
        version,
        checksum: r.i32(H_CHECKSUM)?,
        length: r.i32(H_LENGTH)?,
        flags: r.u32(H_FLAGS)?,
        eye_position: r.floats(H_EYE)?,
        illum_position: r.floats(H_ILLUM)?,
        hull_min: r.floats(H_HULL_MIN)?,
        hull_max: r.floats(H_HULL_MAX)?,
        view_bb_min: r.floats(H_VIEW_MIN)?,
        view_bb_max: r.floats(H_VIEW_MAX)?,
        mass: r.f32(H_MASS)?,
        contents: r.i32(H_CONTENTS)?,
        surface_prop,
        keyvalues,
        root_lod: r.u8(H_ROOT_LOD)?,
        num_allowed_root_lods: r.u8(H_NUM_ALLOWED_ROOT_LODS)?,
        num_bones: count(&r, H_NUM_BONES, "bone count")?,
        num_textures: count(&r, H_NUM_TEXTURES, "texture count")?,
        num_cd_textures: count(&r, H_NUM_CD_TEXTURES, "cdmaterials count")?,
        num_skin_refs: count(&r, H_NUM_SKIN_REF, "skin reference count")?,
        num_skin_families: count(&r, H_NUM_SKIN_FAMILIES, "skin family count")?,
        num_body_parts: count(&r, H_NUM_BODYPARTS, "body part count")?,
        num_sequences: count(&r, H_NUM_LOCAL_SEQ, "sequence count")?,
        num_animations: count(&r, H_NUM_LOCAL_ANIM, "animation count")?,
        num_attachments: count(&r, H_NUM_ATTACHMENTS, "attachment count")?,
        num_include_models: count(&r, H_NUM_INCLUDE_MODELS, "include model count")?,
        num_flex_controllers: count(&r, H_NUM_FLEX_CONTROLLERS, "flex controller count")?,
        num_pose_parameters: count(&r, H_NUM_POSE_PARAMS, "pose parameter count")?,
    })
}

pub(crate) fn parse(data: &[u8]) -> Result<Studio> {
    let info = parse_header(data)?;
    let r = Reader::new(data, FileKind::Mdl);

    let bones = parse_bones(&r)?;
    let textures = parse_textures(&r)?;
    let cd_textures = parse_cd_textures(&r)?;
    let skins = parse_skins(&r)?;
    let body_parts = parse_body_parts(&r)?;
    let include_models = parse_include_models(&r)?;
    let sequences = parse_sequences(&r).unwrap_or_default();
    let hitbox_sets = parse_hitbox_sets(&r).unwrap_or_default();
    let hitboxes = hitbox_sets.first().map(|set| set.hitboxes.clone()).unwrap_or_default();
    let attachments = parse_attachments(&r).unwrap_or_default();

    Ok(Studio {
        info,
        bones,
        textures,
        cd_textures,
        skins,
        body_parts,
        include_models,
        sequences,
        hitboxes,
        hitbox_sets,
        attachments,
    })
}

const ATTACHMENT_SIZE: usize = 92;

fn parse_attachments(r: &Reader<'_>) -> Result<Vec<Attachment>> {
    let (start, n) =
        r.rel_table(0, r.i32(H_NUM_ATTACHMENTS + 4)?, r.i32(H_NUM_ATTACHMENTS)?, ATTACHMENT_SIZE, "attachment table")?;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let a = start + i * ATTACHMENT_SIZE;
        let m: [f32; 12] = r.floats(a + 12)?;
        out.push(Attachment {
            name: rel_str(r, a, r.i32(a)?, "attachment name").unwrap_or_default(),
            bone: r.i32(a + 8)?,
            local: [[m[0], m[1], m[2], m[3]], [m[4], m[5], m[6], m[7]], [m[8], m[9], m[10], m[11]]],
        });
    }
    Ok(out)
}

const HITBOX_SET_SIZE: usize = 12;
const HITBOX_SIZE: usize = 68;

fn parse_hitbox_sets(r: &Reader<'_>) -> Result<Vec<HitboxSet>> {
    let (first, sets) =
        r.rel_table(0, r.i32(H_NUM_HITBOX_SETS + 4)?, r.i32(H_NUM_HITBOX_SETS)?, HITBOX_SET_SIZE, "hitbox set table")?;
    let mut out = Vec::with_capacity(sets);
    for index in 0..sets {
        let set = first + index * HITBOX_SET_SIZE;
        let name_rel = r.i32(set)?;
        let name = if name_rel == 0 { String::new() } else { rel_str(r, set, name_rel, "hitbox set name")? };
        let (start, n) = r.rel_table(set, r.i32(set + 8)?, r.i32(set + 4)?, HITBOX_SIZE, "hitbox table")?;
        let mut hitboxes = Vec::with_capacity(n);
        for i in 0..n {
            let b = start + i * HITBOX_SIZE;
            let name_rel = r.i32(b + 32)?;
            hitboxes.push(Hitbox {
                bone: r.i32(b)?,
                group: r.i32(b + 4)?,
                min: r.floats(b + 8)?,
                max: r.floats(b + 20)?,
                name: if name_rel == 0 {
                    String::new()
                } else {
                    rel_str(r, b, name_rel, "hitbox name").unwrap_or_default()
                },
            });
        }
        out.push(HitboxSet { name, hitboxes });
    }
    Ok(out)
}

fn parse_bones(r: &Reader<'_>) -> Result<Vec<Bone>> {
    let (start, n) = r.rel_table(0, r.i32(H_NUM_BONES + 4)?, r.i32(H_NUM_BONES)?, BONE_SIZE, "bone table")?;
    let mut bones = Vec::with_capacity(n);
    for i in 0..n {
        let b = start + i * BONE_SIZE;
        let m: [f32; 12] = r.floats(b + 96)?;
        bones.push(Bone {
            name: rel_str(r, b, r.i32(b)?, "bone name")?,
            parent: r.i32(b + 4)?,
            pos: r.floats(b + 32)?,
            quat: r.floats(b + 44)?,
            rot: r.floats(b + 60)?,
            pos_scale: r.floats(b + 72)?,
            rot_scale: r.floats(b + 84)?,
            pose_to_bone: [[m[0], m[1], m[2], m[3]], [m[4], m[5], m[6], m[7]], [m[8], m[9], m[10], m[11]]],
            flags: r.u32(b + 160)?,
            surface_prop: rel_str(r, b, r.i32(b + 176)?, "bone surface prop")?,
        });
    }
    Ok(bones)
}

pub(crate) fn parse_textures(r: &Reader<'_>) -> Result<Vec<String>> {
    let (start, n) =
        r.rel_table(0, r.i32(H_NUM_TEXTURES + 4)?, r.i32(H_NUM_TEXTURES)?, TEXTURE_SIZE, "texture table")?;
    (0..n)
        .map(|i| {
            let t = start + i * TEXTURE_SIZE;
            rel_str(r, t, r.i32(t)?, "texture name")
        })
        .collect()
}

pub(crate) fn parse_cd_textures(r: &Reader<'_>) -> Result<Vec<String>> {
    let (start, n) = r.rel_table(
        0,
        r.i32(H_NUM_CD_TEXTURES + 4)?,
        capped(r, r.i32(H_NUM_CD_TEXTURES)?, MAX_CD_TEXTURES, "cdmaterials count")?,
        4,
        "cdmaterials table",
    )?;
    (0..n).map(|i| rel_str(r, 0, r.i32(start + i * 4)?, "cdmaterials path")).collect()
}

pub(crate) fn parse_skins(r: &Reader<'_>) -> Result<Vec<Vec<u16>>> {
    let refs = r.i32(H_NUM_SKIN_REF)?;
    let refs_n = usize::try_from(refs).map_err(|_| r.invalid("skin reference count", i64::from(refs)))?;
    let families = capped(r, r.i32(H_NUM_SKIN_FAMILIES)?, MAX_SKIN_FAMILIES, "skin family count")?;
    let row = refs_n.checked_mul(2).ok_or_else(|| r.invalid("skin reference count", i64::from(refs)))?;
    if row == 0 {
        return Ok(Vec::new());
    }
    let (start, n) = r.rel_table(0, r.i32(H_SKIN_INDEX)?, families, row, "skin table")?;
    let mut skins = Vec::with_capacity(n);
    for f in 0..n {
        let base = start + f * row;
        let mut family = Vec::with_capacity(refs_n);
        for i in 0..refs_n {
            family.push(r.u16(base + i * 2)?);
        }
        skins.push(family);
    }
    Ok(skins)
}

fn parse_body_parts(r: &Reader<'_>) -> Result<Vec<RawBodyPart>> {
    let (start, n) =
        r.rel_table(0, r.i32(H_NUM_BODYPARTS + 4)?, r.i32(H_NUM_BODYPARTS)?, BODYPART_SIZE, "body part table")?;
    let mut parts = Vec::with_capacity(n);
    for i in 0..n {
        let bp = start + i * BODYPART_SIZE;
        let (models_start, num_models) = r.rel_table(bp, r.i32(bp + 12)?, r.i32(bp + 4)?, MODEL_SIZE, "model table")?;
        let mut models = Vec::with_capacity(num_models);
        for m in 0..num_models {
            models.push(parse_model(r, models_start + m * MODEL_SIZE)?);
        }
        parts.push(RawBodyPart { name: rel_str(r, bp, r.i32(bp)?, "body part name")?, base: r.i32(bp + 8)?, models });
    }
    Ok(parts)
}

fn parse_model(r: &Reader<'_>, m: usize) -> Result<RawModel> {
    let (mesh_start, num_meshes) = r.rel_table(m, r.i32(m + 76)?, r.i32(m + 72)?, MESH_SIZE, "mesh table")?;
    let mut meshes = Vec::with_capacity(num_meshes);
    for i in 0..num_meshes {
        let me = mesh_start + i * MESH_SIZE;
        meshes.push(RawMesh { material: r.i32(me)?, num_vertices: r.i32(me + 8)?, vertex_offset: r.i32(me + 12)? });
    }
    Ok(RawModel {
        name: r.fixed_str(m, 64, "model name")?,
        num_vertices: r.i32(m + 80)?,
        vertex_index: r.i32(m + 84)?,
        meshes,
    })
}

pub(crate) fn parse_include_models(r: &Reader<'_>) -> Result<Vec<String>> {
    let (start, n) = r.rel_table(
        0,
        r.i32(H_NUM_INCLUDE_MODELS + 4)?,
        capped(r, r.i32(H_NUM_INCLUDE_MODELS)?, MAX_INCLUDE_MODELS, "include model count")?,
        MODELGROUP_SIZE,
        "include model table",
    )?;
    (0..n)
        .map(|i| {
            let g = start + i * MODELGROUP_SIZE;
            rel_str(r, g, r.i32(g + 4)?, "include model name")
        })
        .collect()
}

fn parse_sequences(r: &Reader<'_>) -> Result<Vec<Sequence>> {
    let (start, n) =
        r.rel_table(0, r.i32(H_NUM_LOCAL_SEQ + 4)?, r.i32(H_NUM_LOCAL_SEQ)?, SEQDESC_SIZE, "sequence table")?;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let s = start + i * SEQDESC_SIZE;
        let baseptr = r.i32(s)?;
        if i64::from(baseptr) != -(s as i64) {
            return Err(r.invalid("sequence baseptr", i64::from(baseptr)));
        }
        out.push(Sequence {
            name: rel_str(r, s, r.i32(s + 4)?, "sequence name")?,
            activity: rel_str(r, s, r.i32(s + 8)?, "activity name")?,
            activity_weight: r.i32(s + 20)?,
            flags: r.u32(s + 12)?,
            bbox_min: r.floats(s + 32)?,
            bbox_max: r.floats(s + 44)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[derive(Default)]
    pub(crate) struct Buf(pub Vec<u8>);

    impl Buf {
        pub(crate) fn at(&mut self, off: usize, bytes: &[u8]) {
            if self.0.len() < off + bytes.len() {
                self.0.resize(off + bytes.len(), 0);
            }
            self.0[off..off + bytes.len()].copy_from_slice(bytes);
        }
        pub(crate) fn i32(&mut self, off: usize, v: i32) {
            self.at(off, &v.to_le_bytes());
        }
        pub(crate) fn u16(&mut self, off: usize, v: u16) {
            self.at(off, &v.to_le_bytes());
        }
        pub(crate) fn f32s(&mut self, off: usize, v: &[f32]) {
            for (i, f) in v.iter().enumerate() {
                self.at(off + i * 4, &f.to_le_bytes());
            }
        }
        pub(crate) fn str(&mut self, off: usize, s: &str) {
            self.at(off, s.as_bytes());
            self.at(off + s.len(), &[0]);
        }
    }

    fn minimal_header() -> Buf {
        let mut b = Buf::default();
        b.at(0, MAGIC);
        b.i32(H_VERSION, 48);
        b.str(H_NAME, "test/box.mdl");
        b.at(HEADER_SIZE, &[0; 64]);
        b
    }

    #[test]
    fn header_rejects_bad_input() {
        assert!(matches!(parse_header(b"IDS"), Err(MdlError::OutOfBounds { .. })));
        assert!(matches!(parse_header(b"XXXX\x30\0\0\0"), Err(MdlError::BadMagic { .. })));
        let mut b = minimal_header();
        b.i32(H_VERSION, 53);
        assert!(matches!(parse_header(&b.0), Err(MdlError::UnsupportedVersion { version: 53, .. })));
        let mut b = minimal_header();
        b.i32(H_NUM_BONES, -5);
        assert!(matches!(parse_header(&b.0), Err(MdlError::InvalidValue { .. })));
        let b = minimal_header();
        assert!(parse_header(&b.0[..300]).is_err());
    }

    #[test]
    fn header_and_tables() {
        let mut b = minimal_header();
        b.f32s(H_HULL_MIN, &[-1.0, -2.0, -3.0]);
        b.f32s(H_HULL_MAX, &[1.0, 2.0, 3.0]);
        b.i32(H_FLAGS, FLAG_STATIC_PROP as i32);
        b.i32(H_NUM_TEXTURES, 1);
        b.i32(H_NUM_TEXTURES + 4, 500);
        b.i32(500, 100);
        b.at(564, &[0; 8]);
        b.str(600, "wood");
        b.i32(H_NUM_CD_TEXTURES, 2);
        b.i32(H_NUM_CD_TEXTURES + 4, 620);
        b.i32(620, 640);
        b.i32(624, 660);
        b.str(640, "models\\props\\");
        b.str(660, "");
        b.i32(H_NUM_SKIN_REF, 1);
        b.i32(H_NUM_SKIN_FAMILIES, 2);
        b.i32(H_SKIN_INDEX, 700);
        b.u16(700, 0);
        b.u16(702, 0);
        b.str(720, "wood_solid");
        b.i32(H_SURFACEPROP, 720);

        let info = parse_header(&b.0).expect("header");
        assert_eq!(info.name, "test/box.mdl");
        assert!(info.is_static_prop());
        assert_eq!(info.bounds(), ([-1.0, -2.0, -3.0], [1.0, 2.0, 3.0]));
        assert_eq!(info.surface_prop, "wood_solid");

        let studio = parse(&b.0).expect("studio");
        assert_eq!(studio.textures, vec!["wood".to_string()]);
        assert_eq!(studio.cd_textures, vec!["models\\props\\".to_string(), String::new()]);
        assert_eq!(studio.skins, vec![vec![0u16], vec![0u16]]);

        b.i32(H_NUM_TEXTURES, 1000);
        assert!(matches!(parse(&b.0), Err(MdlError::OutOfBounds { .. })));
    }
}
