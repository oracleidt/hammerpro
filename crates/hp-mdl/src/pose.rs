use crate::error::FileKind;
use crate::mdl::{Bone, H_NUM_LOCAL_ANIM, H_NUM_LOCAL_SEQ, SEQDESC_SIZE, Studio};
use crate::reader::Reader;
use crate::vvd::BoneWeight;

pub type SkinMatrix = [[f32; 4]; 3];

pub(crate) type Mat34 = SkinMatrix;

pub(crate) const IDENTITY: Mat34 = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];

const ANIMDESC_SIZE: usize = 100;
const MAX_ANIM_RECORDS: usize = 1024;

const ANIM_RAWPOS: u8 = 0x01;
const ANIM_RAWROT: u8 = 0x02;
const ANIM_ANIMPOS: u8 = 0x04;
const ANIM_ANIMROT: u8 = 0x08;
const ANIM_DELTA: u8 = 0x10;
const ANIM_RAWROT2: u8 = 0x20;

const STUDIO_LOOPING: u32 = 0x0001;
const STUDIO_DELTA: u32 = 0x0004;
const STUDIO_ALLZEROS: u32 = 0x0020;

const BONE_HAS_SAVEFRAME_POS: u32 = 0x0020_0000;
const BONE_HAS_SAVEFRAME_ROT: u32 = 0x0040_0000;
const BONE_HAS_SAVEFRAME_ROT32: u32 = 0x0080_0000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pose {
    #[default]
    FirstSequence,
    Sequence(usize),
    Rest,
    Bind,
}

pub(crate) fn mul(a: &Mat34, b: &Mat34) -> Mat34 {
    let mut out = [[0.0f32; 4]; 3];
    for (i, row) in out.iter_mut().enumerate() {
        for (j, cell) in row.iter_mut().enumerate() {
            let mut v = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
            if j == 3 {
                v += a[i][3];
            }
            *cell = v;
        }
    }
    out
}

pub(crate) fn quat_matrix(q: [f32; 4], p: [f32; 3]) -> Mat34 {
    let [x, y, z, w] = q;
    [
        [1.0 - 2.0 * y * y - 2.0 * z * z, 2.0 * x * y - 2.0 * w * z, 2.0 * x * z + 2.0 * w * y, p[0]],
        [2.0 * x * y + 2.0 * w * z, 1.0 - 2.0 * x * x - 2.0 * z * z, 2.0 * y * z - 2.0 * w * x, p[1]],
        [2.0 * x * z - 2.0 * w * y, 2.0 * y * z + 2.0 * w * x, 1.0 - 2.0 * x * x - 2.0 * y * y, p[2]],
    ]
}

pub(crate) fn angle_quaternion(a: [f32; 3]) -> [f32; 4] {
    let (sy, cy) = (a[2] * 0.5).sin_cos();
    let (sp, cp) = (a[1] * 0.5).sin_cos();
    let (sr, cr) = (a[0] * 0.5).sin_cos();
    let (sr_cp, cr_sp) = (sr * cp, cr * sp);
    let (cr_cp, sr_sp) = (cr * cp, sr * sp);
    [sr_cp * cy - cr_sp * sy, cr_sp * cy + sr_cp * sy, cr_cp * sy - sr_sp * cy, cr_cp * cy + sr_sp * sy]
}

fn normalize_quat(q: [f32; 4]) -> [f32; 4] {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if len > 1e-8 && len.is_finite() { [q[0] / len, q[1] / len, q[2] / len, q[3] / len] } else { [0.0, 0.0, 0.0, 1.0] }
}

pub(crate) fn half_to_f32(h: u16) -> f32 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let exp = i32::from((h >> 10) & 0x1f);
    let mant = f32::from(h & 0x3ff);
    match exp {
        0 => sign * mant * (2.0f32).powi(-24),
        31 => {
            if mant == 0.0 {
                sign * f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => sign * (1.0 + mant / 1024.0) * (2.0f32).powi(exp - 15),
    }
}

pub(crate) fn quat48(b: [u8; 6]) -> [f32; 4] {
    let x = (f32::from(u16::from_le_bytes([b[0], b[1]])) - 32768.0) / 32768.0;
    let y = (f32::from(u16::from_le_bytes([b[2], b[3]])) - 32768.0) / 32768.0;
    let zw = u16::from_le_bytes([b[4], b[5]]);
    let z = (f32::from(zw & 0x7fff) - 16384.0) / 16384.0;
    let w = (1.0 - x * x - y * y - z * z).max(0.0).sqrt();
    [x, y, z, if zw & 0x8000 != 0 { -w } else { w }]
}

pub(crate) fn quat64(b: [u8; 8]) -> [f32; 4] {
    let v = u64::from_le_bytes(b);
    let comp = |shift: u32| ((((v >> shift) & 0x1f_ffff) as f64 - 1_048_576.0) / 1_048_576.5) as f32;
    let (x, y, z) = (comp(0), comp(21), comp(42));
    let w = (1.0 - x * x - y * y - z * z).max(0.0).sqrt();
    [x, y, z, if v >> 63 != 0 { -w } else { w }]
}

fn vec48(b: [u8; 6]) -> [f32; 3] {
    [
        half_to_f32(u16::from_le_bytes([b[0], b[1]])),
        half_to_f32(u16::from_le_bytes([b[2], b[3]])),
        half_to_f32(u16::from_le_bytes([b[4], b[5]])),
    ]
}

fn extract_anim_value(r: &Reader<'_>, mut at: usize, mut k: usize) -> Option<(f32, f32)> {
    for _ in 0..MAX_ANIM_RECORDS {
        let valid = usize::from(r.u8(at).ok()?);
        let total = usize::from(r.u8(at + 1).ok()?);
        if total == 0 {
            return Some((0.0, 0.0));
        }
        if total > k {
            let read = |idx: usize| r.i16(at + idx * 2).ok().map(f32::from);
            let v1 = if valid > k { read(k + 1)? } else { read(valid)? };
            let v2 = if valid > k + 1 {
                read(k + 2).unwrap_or(v1)
            } else if total > k + 1 {
                v1
            } else {
                read(valid + 2).unwrap_or(v1)
            };
            return Some((v1, v2));
        }
        k -= total;
        at += (valid + 1) * 2;
    }
    None
}

fn anim_values(r: &Reader<'_>, ptr: usize, frame: usize) -> Option<([f32; 3], [f32; 3])> {
    let mut a = [0.0f32; 3];
    let mut b = [0.0f32; 3];
    for i in 0..3 {
        let offset = r.i16(ptr + i * 2).ok()?;
        if offset > 0 {
            let (v1, v2) = extract_anim_value(r, ptr + offset as usize, frame)?;
            a[i] = v1;
            b[i] = v2;
        }
    }
    Some((a, b))
}

fn quat_blend(p: [f32; 4], q: [f32; 4], t: f32) -> [f32; 4] {
    let dot = p[0] * q[0] + p[1] * q[1] + p[2] * q[2] + p[3] * q[3];
    let q = if dot < 0.0 { [-q[0], -q[1], -q[2], -q[3]] } else { q };
    let u = 1.0 - t;
    normalize_quat([u * p[0] + t * q[0], u * p[1] + t * q[1], u * p[2] + t * q[2], u * p[3] + t * q[3]])
}

fn read_array<const N: usize>(r: &Reader<'_>, at: usize) -> Option<[u8; N]> {
    r.bytes(at, N, "animation").ok()?.try_into().ok()
}

const H_ANIMBLOCK_NAME: usize = 348;
const H_NUM_ANIMBLOCKS: usize = 352;
const H_ANIMBLOCK_INDEX: usize = 356;
const MAX_ANIM_BLOCKS: i32 = 1 << 16;

fn ani_block_data(r: &Reader<'_>, ani: &Reader<'_>, block: i32, index: i32) -> Option<usize> {
    let count = r.i32(H_NUM_ANIMBLOCKS).ok()?;
    if block <= 0 || block >= count || count > MAX_ANIM_BLOCKS || index < 0 {
        return None;
    }
    let table = r.rel(0, r.i32(H_ANIMBLOCK_INDEX).ok()?, "animation blocks").ok()?;
    let entry = table.checked_add(usize::try_from(block).ok()?.checked_mul(8)?)?;
    let start = usize::try_from(r.i32(entry).ok()?).ok()?;
    let end = usize::try_from(r.i32(entry + 4).ok()?).ok()?;
    let at = start.checked_add(usize::try_from(index).ok()?)?;
    (at < end).then_some(())?;
    ani.check(at, 4, "animation block data").ok()?;
    Some(at)
}

pub fn animation_block_path(mdl: &[u8]) -> Option<String> {
    let r = Reader::new(mdl, FileKind::Mdl);
    let count = r.i32(H_NUM_ANIMBLOCKS).ok()?;
    if count <= 1 || count > MAX_ANIM_BLOCKS {
        return None;
    }
    let name_at = r.rel(0, r.i32(H_ANIMBLOCK_NAME).ok()?, "animation block name").ok()?;
    let name = r.cstr(name_at, "animation block name").ok()?.replace('\\', "/");
    name.to_ascii_lowercase().ends_with(".ani").then_some(name)
}

pub(crate) fn seq_anim(r: &Reader<'_>, seq: usize, blend: usize) -> Option<(usize, usize)> {
    let num_seq = usize::try_from(r.i32(H_NUM_LOCAL_SEQ).ok()?).ok()?;
    if seq >= num_seq {
        return None;
    }
    let seq_at =
        r.rel(0, r.i32(H_NUM_LOCAL_SEQ + 4).ok()?, "sequence").ok()?.checked_add(seq.checked_mul(SEQDESC_SIZE)?)?;
    r.check(seq_at, SEQDESC_SIZE, "sequence").ok()?;
    let blends = usize::try_from(r.i32(seq_at + 56).ok()?).ok()?.max(1);
    if blend >= blends {
        return None;
    }
    let blend_at = r.rel(seq_at, r.i32(seq_at + 60).ok()?, "blend table").ok()?.checked_add(blend * 2)?;
    let anim = usize::try_from(r.i16(blend_at).ok()?).ok()?;
    let num_anim = usize::try_from(r.i32(H_NUM_LOCAL_ANIM).ok()?).ok()?;
    if anim >= num_anim {
        return None;
    }
    let desc =
        r.rel(0, r.i32(H_NUM_LOCAL_ANIM + 4).ok()?, "animation").ok()?.checked_add(anim.checked_mul(ANIMDESC_SIZE)?)?;
    r.check(desc, ANIMDESC_SIZE, "animation").ok()?;
    Some((seq_at, desc))
}

pub(crate) fn anim_timing(r: &Reader<'_>, seq: usize, blend: usize) -> Option<(usize, f32, bool)> {
    let (seq_at, desc) = seq_anim(r, seq, blend)?;
    let fps = r.f32(desc + 8).ok()?;
    let frames = usize::try_from(r.i32(desc + 16).ok()?).ok()?;
    let looping = r.u32(seq_at + 12).ok()? & STUDIO_LOOPING != 0;
    (frames >= 1 && fps.is_finite() && fps > 0.0).then_some((frames, fps, looping))
}

pub(crate) fn sequence_pose(
    r: &Reader<'_>,
    ani: Option<&Reader<'_>>,
    bones: &[Bone],
    (seq, blend): (usize, usize),
    mut frame: usize,
    mut s: f32,
) -> Option<Vec<([f32; 3], [f32; 4])>> {
    let (seq_at, desc) = seq_anim(r, seq, blend)?;
    if r.u32(seq_at + 12).ok()? & STUDIO_DELTA != 0 {
        return None;
    }
    let flags = r.u32(desc + 12).ok()?;
    let frames = usize::try_from(r.i32(desc + 16).ok()?).ok()?.max(1);
    if frame + 1 >= frames {
        frame = frames.saturating_sub(1);
        s = 0.0;
    }

    let mut pose: Vec<([f32; 3], [f32; 4])> = bones.iter().map(|b| (b.pos, b.quat)).collect();
    if flags & STUDIO_DELTA != 0 {
        return None;
    }
    if flags & STUDIO_ALLZEROS != 0 {
        return Some(pose);
    }
    let weights_rel = r.i32(seq_at + 156).ok()?;
    let animated = |bone: usize| -> bool {
        weights_rel == 0
            || r.rel(seq_at, weights_rel, "bone weights")
                .ok()
                .and_then(|w| r.f32(w + bone * 4).ok())
                .is_none_or(|w| w > 0.0)
    };
    let (mut block, mut index) = (r.i32(desc + 52).ok()?, r.i32(desc + 56).ok()?);
    let mut local = frame;
    let section_frames = usize::try_from(r.i32(desc + 84).ok()?).unwrap_or(0);
    if let Some(mut section_index) = frame.checked_div(section_frames) {
        local = frame - section_index * section_frames;
        if frames > section_frames && frame == frames - 1 {
            section_index = frames / section_frames + 1;
            local = 0;
        }
        let section = r
            .rel(desc, r.i32(desc + 80).ok()?, "animation section")
            .ok()?
            .checked_add(section_index.checked_mul(8)?)?;
        block = r.i32(section).ok()?;
        index = r.i32(section + 4).ok()?;
    }
    if block < 0 {
        return None;
    }
    let (ar, mut at) = if block == 0 {
        (r, r.rel(desc, index, "animation data").ok()?)
    } else if let Some(a) = ani
        && let Some(at) = ani_block_data(r, a, block, index)
    {
        (a, at)
    } else {
        return zero_frame(r, bones, desc, pose, animated);
    };
    for _ in 0..MAX_ANIM_RECORDS {
        let bone = usize::from(ar.u8(at).ok()?);
        let aflags = ar.u8(at + 1).ok()?;
        let next = ar.i16(at + 2).ok()?;
        let data = at + 4;
        let b = bones.get(bone)?;
        let delta = aflags & ANIM_DELTA != 0;
        let quat = if aflags & ANIM_RAWROT != 0 {
            quat48(read_array(ar, data)?)
        } else if aflags & ANIM_RAWROT2 != 0 {
            quat64(read_array(ar, data)?)
        } else if aflags & ANIM_ANIMROT != 0 {
            let (v1, v2) = anim_values(ar, data, local)?;
            let angles = |v: [f32; 3]| {
                let mut e = [v[0] * b.rot_scale[0], v[1] * b.rot_scale[1], v[2] * b.rot_scale[2]];
                if !delta {
                    for (c, base) in e.iter_mut().zip(b.rot) {
                        *c += base;
                    }
                }
                e
            };
            let q1 = angle_quaternion(angles(v1));
            if s > 0.0 && v1 != v2 { quat_blend(q1, angle_quaternion(angles(v2)), s) } else { q1 }
        } else if delta {
            [0.0, 0.0, 0.0, 1.0]
        } else {
            b.quat
        };
        let raw_pos_at =
            data + if aflags & ANIM_RAWROT != 0 { 6 } else { 0 } + if aflags & ANIM_RAWROT2 != 0 { 8 } else { 0 };
        let pos = if aflags & ANIM_RAWPOS != 0 {
            vec48(read_array(ar, raw_pos_at)?)
        } else if aflags & ANIM_ANIMPOS != 0 {
            let ptr = data + if aflags & ANIM_ANIMROT != 0 { 6 } else { 0 };
            let (v1, v2) = anim_values(ar, ptr, local)?;
            let v = [v1[0] + (v2[0] - v1[0]) * s, v1[1] + (v2[1] - v1[1]) * s, v1[2] + (v2[2] - v1[2]) * s];
            let mut p = [v[0] * b.pos_scale[0], v[1] * b.pos_scale[1], v[2] * b.pos_scale[2]];
            if !delta {
                for (c, base) in p.iter_mut().zip(b.pos) {
                    *c += base;
                }
            }
            p
        } else if delta {
            [0.0; 3]
        } else {
            b.pos
        };
        if delta {
            return None;
        }
        if animated(bone)
            && let Some(slot) = pose.get_mut(bone)
        {
            *slot = (pos, normalize_quat(quat));
        }
        if next <= 0 {
            return Some(pose);
        }
        at += next as usize;
    }
    None
}

fn zero_frame(
    r: &Reader<'_>,
    bones: &[Bone],
    desc: usize,
    mut pose: Vec<([f32; 3], [f32; 4])>,
    animated: impl Fn(usize) -> bool,
) -> Option<Vec<([f32; 3], [f32; 4])>> {
    let rel = r.i32(desc + 92).ok()?;
    if rel == 0 {
        return None;
    }
    let count = usize::try_from(r.i16(desc + 90).ok()?).ok().filter(|&c| c > 0)?;
    let mut at = r.rel(desc, rel, "zero frame data").ok()?;
    for (i, bone) in bones.iter().enumerate() {
        if bone.flags & BONE_HAS_SAVEFRAME_ROT32 != 0 {
            return None;
        }
        let slot = pose.get_mut(i)?;
        if bone.flags & BONE_HAS_SAVEFRAME_POS != 0 {
            let p = vec48(read_array(r, at)?);
            if animated(i) {
                slot.0 = p;
            }
            at += 6 * count;
        }
        if bone.flags & BONE_HAS_SAVEFRAME_ROT != 0 {
            let q = quat64(read_array(r, at)?);
            if animated(i) {
                slot.1 = normalize_quat(q);
            }
            at += 8 * count;
        }
    }
    Some(pose)
}

pub(crate) struct Posing {
    pub skin: Option<Vec<Mat34>>,
    pub sequence: Option<usize>,
    pub bones: Vec<Mat34>,
}

pub(crate) fn posing(mdl: &[u8], studio: &Studio, pose: Pose) -> Posing {
    let none = Posing {
        skin: None,
        sequence: None,
        bones: studio
            .bones
            .iter()
            .map(|bone| {
                let m = bone.pose_to_bone;
                let mut inverse = IDENTITY;
                for row in 0..3 {
                    for column in 0..3 {
                        inverse[row][column] = m[column][row];
                    }
                    inverse[row][3] =
                        -(inverse[row][0] * m[0][3] + inverse[row][1] * m[1][3] + inverse[row][2] * m[2][3]);
                }
                inverse
            })
            .collect(),
    };
    if studio.info.flags & crate::mdl::FLAG_STATIC_PROP != 0 || studio.bones.is_empty() {
        return none;
    }
    let r = Reader::new(mdl, FileKind::Mdl);
    let wanted = match pose {
        Pose::Bind => return none,
        Pose::Rest => None,
        Pose::FirstSequence => Some(0),
        Pose::Sequence(s) => Some(s),
    };
    let decoded = wanted.and_then(|s| sequence_pose(&r, None, &studio.bones, (s, 0), 0, 0.0).map(|p| (s, p)));
    let sequence = decoded.as_ref().map(|(s, _)| *s);
    let local = decoded.map(|(_, p)| p).unwrap_or_else(|| studio.bones.iter().map(|b| (b.pos, b.quat)).collect());
    let bones = model_from_local(&studio.bones, &local);
    let skin: Vec<_> = bones.iter().zip(&studio.bones).map(|(m, b)| mul(m, &b.pose_to_bone)).collect();
    let identity =
        skin.iter().flatten().flatten().zip(IDENTITY.iter().flatten().cycle()).all(|(a, b)| (a - b).abs() < 1e-5);
    let finite = skin.iter().flatten().flatten().all(|v| v.is_finite());
    Posing { skin: (!identity && finite).then_some(skin), sequence, bones }
}

pub(crate) fn skin_from_local(bones: &[Bone], local: &[([f32; 3], [f32; 4])]) -> Vec<Mat34> {
    model_from_local(bones, local).iter().zip(bones).map(|(m, b)| mul(m, &b.pose_to_bone)).collect()
}

fn model_from_local(bones: &[Bone], local: &[([f32; 3], [f32; 4])]) -> Vec<Mat34> {
    let mut bone_to_model: Vec<Mat34> = Vec::with_capacity(bones.len());
    for (i, (bone, (pos, quat))) in bones.iter().zip(local).enumerate() {
        let m = quat_matrix(*quat, *pos);
        let parent = usize::try_from(bone.parent).ok().filter(|&p| p < i).and_then(|p| bone_to_model.get(p));
        bone_to_model.push(match parent {
            Some(parent) => mul(parent, &m),
            None => m,
        });
    }
    bone_to_model
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimClip {
    pub sequence: usize,
    pub frames: usize,
    pub fps: f32,
    pub looping: bool,
}

impl AnimClip {
    pub fn duration(&self) -> f32 {
        if self.frames <= 1 { 0.0 } else { (self.frames - 1) as f32 / self.fps }
    }

    pub fn frame_at(&self, time_s: f32) -> (usize, f32) {
        if self.frames <= 1 || !time_s.is_finite() {
            return (0, 0.0);
        }
        let last = (self.frames - 1) as f32;
        let mut t = time_s * self.fps;
        if self.looping {
            t = t.rem_euclid(last.max(1.0));
        } else {
            t = t.clamp(0.0, last);
        }
        let frame = t.floor();
        (frame as usize, (t - frame).clamp(0.0, 1.0))
    }
}

pub fn animation_clip(mdl: &[u8], seq: usize) -> Option<AnimClip> {
    let r = Reader::new(mdl, FileKind::Mdl);
    let (frames, fps, looping) = anim_timing(&r, seq, 0)?;
    Some(AnimClip { sequence: seq, frames, fps, looping })
}

pub fn skin_matrices(mdl: &[u8], bones: &[Bone], seq: usize, frame: usize, frac: f32) -> Option<Vec<SkinMatrix>> {
    skin_matrices_with(mdl, None, bones, seq, frame, frac)
}

pub fn skin_matrices_with(
    mdl: &[u8],
    ani: Option<&[u8]>,
    bones: &[Bone],
    seq: usize,
    frame: usize,
    frac: f32,
) -> Option<Vec<SkinMatrix>> {
    let r = Reader::new(mdl, FileKind::Mdl);
    let a = ani.map(|bytes| Reader::new(bytes, FileKind::Mdl));
    let local = sequence_pose(&r, a.as_ref(), bones, (seq, 0), frame, frac.clamp(0.0, 1.0))?;
    Some(skin_from_local(bones, &local))
}

pub fn pose_vertex(
    matrices: &[SkinMatrix],
    weight: &BoneWeight,
    pos: [f32; 3],
    normal: [f32; 3],
) -> ([f32; 3], [f32; 3]) {
    let (p, n, _) = skin_vertex(matrices, weight, pos, normal, None);
    (p, n)
}

fn transform_point(m: &Mat34, p: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
        m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
        m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
    ]
}

fn transform_dir(m: &Mat34, d: [f32; 3]) -> [f32; 3] {
    [
        m[0][0] * d[0] + m[0][1] * d[1] + m[0][2] * d[2],
        m[1][0] * d[0] + m[1][1] * d[1] + m[1][2] * d[2],
        m[2][0] * d[0] + m[2][1] * d[1] + m[2][2] * d[2],
    ]
}

fn normalize3(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len > 1e-12 && len.is_finite() { [v[0] / len, v[1] / len, v[2] / len] } else { fallback }
}

pub(crate) fn skin_vertex(
    skin: &[Mat34],
    w: &BoneWeight,
    pos: [f32; 3],
    normal: [f32; 3],
    tangent: Option<[f32; 4]>,
) -> ([f32; 3], [f32; 3], Option<[f32; 4]>) {
    let mut p = [0.0f32; 3];
    let mut n = [0.0f32; 3];
    let mut t = [0.0f32; 3];
    let mut total = 0.0f32;
    let count = usize::from(w.count.min(3)).max(1);
    for i in 0..count {
        let (Some(&bone), Some(&weight)) = (w.bones.get(i), w.weights.get(i)) else {
            continue;
        };
        let Some(m) = skin.get(usize::from(bone)) else {
            continue;
        };
        let weight = if w.count == 0 { 1.0 } else { weight };
        if weight.is_nan() || weight <= 0.0 {
            continue;
        }
        let tp = transform_point(m, pos);
        let tn = transform_dir(m, normal);
        for k in 0..3 {
            p[k] += tp[k] * weight;
            n[k] += tn[k] * weight;
        }
        if let Some(tan) = tangent {
            let tt = transform_dir(m, [tan[0], tan[1], tan[2]]);
            for k in 0..3 {
                t[k] += tt[k] * weight;
            }
        }
        total += weight;
    }
    if total.is_nan() || total <= 0.0 {
        return (pos, normal, tangent);
    }
    let p = [p[0] / total, p[1] / total, p[2] / total];
    let n = normalize3(n, normal);
    let tangent = tangent.map(|tan| {
        let t = normalize3(t, [tan[0], tan[1], tan[2]]);
        [t[0], t[1], t[2], tan[3]]
    });
    (p, n, tangent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f32], b: &[f32]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn math() {
        let q = angle_quaternion([0.0, 0.0, std::f32::consts::FRAC_PI_2]);
        let m = quat_matrix(q, [1.0, 2.0, 3.0]);
        assert!(close(&transform_point(&m, [1.0, 0.0, 0.0]), &[1.0, 3.0, 3.0]));
        assert!(close(&transform_dir(&m, [0.0, 1.0, 0.0]), &[-1.0, 0.0, 0.0]));
        let inv = quat_matrix([-q[0], -q[1], -q[2], q[3]], [0.0; 3]);
        let id = mul(&quat_matrix(q, [0.0; 3]), &inv);
        assert!(close(id.as_flattened(), IDENTITY.as_flattened()));
    }

    #[test]
    fn packed_formats() {
        assert_eq!(half_to_f32(0x3c00), 1.0);
        assert_eq!(half_to_f32(0xc000), -2.0);
        assert_eq!(half_to_f32(0x0000), 0.0);
        assert!(half_to_f32(0x7c00).is_infinite());
        let q = quat48([0x00, 0x80, 0x00, 0x80, 0x00, 0x40]);
        assert!(close(&q, &[0.0, 0.0, 0.0, 1.0]));
        let mid = 1u64 << 20;
        let packed = mid | (mid << 21) | (mid << 42) | (1 << 63);
        assert!(close(&quat64(packed.to_le_bytes()), &[0.0, 0.0, 0.0, -1.0]));
    }

    fn runs(runs: &[(u8, u8, &[i16])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (valid, total, samples) in runs {
            out.push(*valid);
            out.push(*total);
            for s in *samples {
                out.extend_from_slice(&s.to_le_bytes());
            }
        }
        out
    }

    #[test]
    fn extract_walks_runs_and_blends() {
        let data = runs(&[(4, 4, &[10, 20, 30, 40])]);
        let r = Reader::new(&data, FileKind::Mdl);
        assert_eq!(extract_anim_value(&r, 0, 0), Some((10.0, 20.0)));
        assert_eq!(extract_anim_value(&r, 0, 2), Some((30.0, 40.0)));
        assert_eq!(extract_anim_value(&r, 0, 3), Some((40.0, 40.0)));
        let data = runs(&[(2, 5, &[7, 9])]);
        let r = Reader::new(&data, FileKind::Mdl);
        assert_eq!(extract_anim_value(&r, 0, 0), Some((7.0, 9.0)));
        assert_eq!(extract_anim_value(&r, 0, 1), Some((9.0, 9.0)), "beyond valid, before total: holds");
        assert_eq!(extract_anim_value(&r, 0, 3), Some((9.0, 9.0)));
        let data = runs(&[(2, 2, &[1, 2]), (2, 2, &[3, 4])]);
        let r = Reader::new(&data, FileKind::Mdl);
        assert_eq!(extract_anim_value(&r, 0, 0), Some((1.0, 2.0)));
        assert_eq!(extract_anim_value(&r, 0, 1), Some((2.0, 3.0)));
        assert_eq!(extract_anim_value(&r, 0, 2), Some((3.0, 4.0)));
    }

    #[test]
    fn clip_frame_timing() {
        let clip = AnimClip { sequence: 0, frames: 11, fps: 10.0, looping: false };
        assert!((clip.duration() - 1.0).abs() < 1e-6);
        assert_eq!(clip.frame_at(0.0), (0, 0.0));
        let (f, s) = clip.frame_at(0.25);
        assert_eq!(f, 2);
        assert!((s - 0.5).abs() < 1e-5, "{s}");
        assert_eq!(clip.frame_at(5.0), (10, 0.0));
        let loop_clip = AnimClip { looping: true, ..clip };
        assert_eq!(loop_clip.frame_at(1.0), (0, 0.0));
        let (f, _) = loop_clip.frame_at(1.25);
        assert_eq!(f, 2);
        let still = AnimClip { sequence: 1, frames: 1, fps: 30.0, looping: true };
        assert_eq!(still.duration(), 0.0);
        assert_eq!(still.frame_at(99.0), (0, 0.0));
    }

    #[test]
    fn skinning_blends_and_ignores_bad_bones() {
        let shift = [[1.0, 0.0, 0.0, 10.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
        let skin = [IDENTITY, shift];
        let w = BoneWeight { weights: [0.5, 0.5, 0.0], bones: [0, 1, 0], count: 2 };
        let (p, n, _) = skin_vertex(&skin, &w, [0.0; 3], [0.0, 0.0, 1.0], None);
        assert!(close(&p, &[5.0, 0.0, 0.0]) && close(&n, &[0.0, 0.0, 1.0]));
        let bad = BoneWeight { weights: [1.0, 0.0, 0.0], bones: [9, 0, 0], count: 1 };
        let (p, _, _) = skin_vertex(&skin, &bad, [1.0, 2.0, 3.0], [0.0, 0.0, 1.0], None);
        assert!(close(&p, &[1.0, 2.0, 3.0]));
    }
}
