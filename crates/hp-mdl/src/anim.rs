use crate::error::FileKind;
use crate::mdl::{Bone, SEQDESC_SIZE, Sequence};
use crate::pose::{
    AnimClip, LocalPose, SkinMatrix, anim_timing, decode_pose, quat_blend, seq_anim, sequence_pose, skin_from_local,
};
use crate::reader::Reader;

const H_NUM_POSE_PARAMS: usize = 300;
const POSE_PARAM_SIZE: usize = 20;
const MAX_SOURCES: usize = 16;
const SEQ_FLAGS: usize = 12;
const SEQ_NUM_AUTOLAYERS: usize = 148;
const SEQ_AUTOLAYER_INDEX: usize = 152;
const SEQ_WEIGHTLIST_INDEX: usize = 156;
const AUTOLAYER_SIZE: usize = 24;
const MAX_LAYERS: usize = 16;
const MAX_LAYER_DEPTH: usize = 3;
const STUDIO_DELTA: u32 = 0x0004;
const STUDIO_AUTOPLAY: u32 = 0x0008;
const STUDIO_POST: u32 = 0x0010;
const AL_SPLINE: i32 = 0x0040;
const AL_XFADE: i32 = 0x0080;
const AL_NOBLEND: i32 = 0x0200;
const AL_LOCAL: i32 = 0x1000;
const AL_POSE: i32 = 0x4000;

struct Source {
    mdl: Vec<u8>,
    ani: Option<Vec<u8>>,
    bones: Vec<Bone>,
    map: Vec<Option<usize>>,
    sequences: Vec<String>,
    infos: Vec<Sequence>,
}

pub struct AnimSet {
    bones: Vec<Bone>,
    sources: Vec<Source>,
    order: Vec<(usize, usize)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimRef {
    source: usize,
    sequence: usize,
    blend: usize,
    pub clip: AnimClip,
}

impl AnimSet {
    pub fn load(mdl: &[u8], read: impl Fn(&str) -> Option<Vec<u8>>) -> Option<Self> {
        let studio = crate::mdl::parse(mdl).ok()?;
        if studio.bones.is_empty() {
            return None;
        }
        let bones = studio.bones.clone();
        let mut sources = vec![Source {
            ani: crate::pose::animation_block_path(mdl).and_then(|p| read(&p)),
            map: (0..bones.len()).map(Some).collect(),
            sequences: studio.sequences.iter().map(|s| s.name.to_ascii_lowercase()).collect(),
            infos: studio.sequences,
            bones: studio.bones,
            mdl: mdl.to_vec(),
        }];
        for name in studio.include_models.iter().take(MAX_SOURCES) {
            let Some(path) = crate::deps::include_path(name) else { continue };
            let Some(data) = read(&path) else { continue };
            let Ok(inc) = crate::mdl::parse(&data) else { continue };
            let map =
                inc.bones.iter().map(|b| bones.iter().position(|t| t.name.eq_ignore_ascii_case(&b.name))).collect();
            sources.push(Source {
                ani: crate::pose::animation_block_path(&data).and_then(|p| read(&p)),
                sequences: inc.sequences.iter().map(|s| s.name.to_ascii_lowercase()).collect(),
                infos: inc.sequences,
                bones: inc.bones,
                map,
                mdl: data,
            });
        }
        let mut order: Vec<(usize, usize)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (i, src) in sources.iter().enumerate() {
            for (q, name) in src.sequences.iter().enumerate() {
                if seen.insert(name.clone()) {
                    order.push((i, q));
                }
            }
        }
        Some(Self { bones, sources, order })
    }

    pub fn sequence_count(&self) -> usize {
        self.order.len()
    }

    pub fn sequence_info(&self, index: usize) -> Option<&Sequence> {
        let &(source, sequence) = self.order.get(index)?;
        self.sources.get(source)?.infos.get(sequence)
    }

    pub fn sequence_index(&self, name: &str) -> Option<usize> {
        let name = name.trim().to_ascii_lowercase();
        self.order.iter().position(|&(s, q)| self.sources[s].sequences.get(q).is_some_and(|n| *n == name))
    }

    pub fn sequence_at(&self, index: usize, params: &[(&str, f32)]) -> Option<AnimRef> {
        let &(source, sequence) = self.order.get(index)?;
        let src = self.sources.get(source)?;
        let r = Reader::new(&src.mdl, FileKind::Mdl);
        let blend = blend_cell(&r, sequence, params).unwrap_or(0);
        let (frames, fps, looping) = anim_timing(&r, sequence, blend)?;
        Some(AnimRef { source, sequence, blend, clip: AnimClip { sequence: index, frames, fps, looping } })
    }

    pub fn frames(&self, index: usize) -> usize {
        self.sequence_at(index, &[]).map_or(1, |a| a.clip.frames.max(1))
    }

    pub fn default_sequence(&self, preferred: Option<&str>, idle_first: bool) -> Option<usize> {
        let moving = |i: usize| self.frames(i) > 1;
        if let Some(i) = preferred.map(str::trim).filter(|p| !p.is_empty()).and_then(|p| self.sequence_index(p)) {
            return Some(i);
        }
        let n = self.sequence_count();
        let idle_activity = || {
            (0..n).find(|&i| {
                moving(i) && self.sequence_info(i).is_some_and(|s| s.activity.eq_ignore_ascii_case("ACT_IDLE"))
            })
        };
        let idle_name = || {
            (0..n)
                .find(|&i| moving(i) && self.sequence_info(i).is_some_and(|s| s.name.eq_ignore_ascii_case("idle")))
                .or_else(|| {
                    (0..n).find(|&i| {
                        moving(i) && self.sequence_info(i).is_some_and(|s| s.name.to_ascii_lowercase().contains("idle"))
                    })
                })
        };
        let first = || (n > 0 && moving(0)).then_some(0);
        if idle_first {
            idle_activity().or_else(idle_name).or_else(first)
        } else {
            first().or_else(idle_activity).or_else(idle_name)
        }
    }

    pub fn bones(&self) -> &[Bone] {
        &self.bones
    }

    pub fn has(&self, name: &str) -> bool {
        let name = name.to_ascii_lowercase();
        self.sources.iter().any(|s| s.sequences.contains(&name))
    }

    pub fn sequence(&self, name: &str, params: &[(&str, f32)]) -> Option<AnimRef> {
        let name = name.to_ascii_lowercase();
        let (source, sequence) = self
            .sources
            .iter()
            .enumerate()
            .find_map(|(i, s)| s.sequences.iter().position(|n| *n == name).map(|q| (i, q)))?;
        let src = &self.sources[source];
        let r = Reader::new(&src.mdl, FileKind::Mdl);
        let blend = blend_cell(&r, sequence, params).unwrap_or(0);
        let (frames, fps, looping) = anim_timing(&r, sequence, blend)?;
        Some(AnimRef { source, sequence, blend, clip: AnimClip { sequence, frames, fps, looping } })
    }

    pub fn skin(&self, anim: &AnimRef, frame: usize, frac: f32) -> Option<Vec<SkinMatrix>> {
        let src = self.sources.get(anim.source)?;
        let r = Reader::new(&src.mdl, FileKind::Mdl);
        let ani = src.ani.as_deref().map(|a| Reader::new(a, FileKind::Mdl));
        let posed =
            sequence_pose(&r, ani.as_ref(), &src.bones, (anim.sequence, anim.blend), frame, frac.clamp(0.0, 1.0))?;
        let mut local: Vec<([f32; 3], [f32; 4])> = self.bones.iter().map(|b| (b.pos, b.quat)).collect();
        for (pose, target) in posed.into_iter().zip(&src.map) {
            if let Some(slot) = target.and_then(|t| local.get_mut(t)) {
                *slot = pose;
            }
        }
        let skin = skin_from_local(&self.bones, &local);
        skin.iter().flatten().flatten().all(|v| v.is_finite()).then_some(skin)
    }

    pub fn skin_layered(&self, anim: &AnimRef, frame: usize, frac: f32, time_s: f32) -> Option<Vec<SkinMatrix>> {
        let src = self.sources.get(anim.source)?;
        let r = Reader::new(&src.mdl, FileKind::Mdl);
        let ani = src.ani.as_deref().map(|a| Reader::new(a, FileKind::Mdl));
        let layers = Layers { r: &r, ani: ani.as_ref(), bones: &src.bones };
        let mut pose: LocalPose = src.bones.iter().map(|b| (b.pos, b.quat)).collect();
        let frames = anim.clip.frames.max(1);
        let cycle = if frames > 1 { (frame as f32 + frac.clamp(0.0, 1.0)) / (frames - 1) as f32 } else { 0.0 };
        let main = layers.pose_at(anim.sequence, anim.blend, cycle)?;
        let mut main = main;
        layers.add_layers(&mut main.0, anim.sequence, cycle, 1.0, true, 0);
        slerp_bones(&r, anim.sequence, &mut pose, &main.0, 1.0, main.1);
        layers.add_layers(&mut pose, anim.sequence, cycle, 1.0, false, 0);
        let autoplay =
            src.infos.iter().enumerate().filter(|(i, q)| *i != anim.sequence && q.flags & STUDIO_AUTOPLAY != 0);
        for (seq, _) in autoplay.take(MAX_LAYERS) {
            layers.add_autoplay(&mut pose, seq, time_s);
        }
        let mut local: Vec<([f32; 3], [f32; 4])> = self.bones.iter().map(|b| (b.pos, b.quat)).collect();
        for (posed, target) in pose.into_iter().zip(&src.map) {
            if let Some(slot) = target.and_then(|t| local.get_mut(t)) {
                *slot = posed;
            }
        }
        let skin = skin_from_local(&self.bones, &local);
        skin.iter().flatten().flatten().all(|v| v.is_finite()).then_some(skin)
    }
}

struct Layers<'a> {
    r: &'a Reader<'a>,
    ani: Option<&'a Reader<'a>>,
    bones: &'a [Bone],
}

impl Layers<'_> {
    fn pose_at(&self, seq: usize, blend: usize, cycle: f32) -> Option<(LocalPose, bool)> {
        let (frames, _, looping) = anim_timing(self.r, seq, blend)?;
        let cycle = if looping { cycle.rem_euclid(1.0) } else { cycle.clamp(0.0, 1.0) };
        let position = cycle * frames.saturating_sub(1) as f32;
        let frame = position.floor().max(0.0) as usize;
        decode_pose(self.r, self.ani, self.bones, (seq, blend), frame, position - frame as f32, true)
    }

    fn accumulate(&self, pose: &mut LocalPose, seq: usize, cycle: f32, weight: f32, depth: usize) {
        if depth >= MAX_LAYER_DEPTH || weight <= 0.0 {
            return;
        }
        let blend = blend_cell(self.r, seq, &[]).unwrap_or(0);
        let Some((mut layer, delta)) = self.pose_at(seq, blend, cycle) else { return };
        self.add_layers(&mut layer, seq, cycle, 1.0, true, depth + 1);
        slerp_bones(self.r, seq, pose, &layer, weight, delta);
        self.add_layers(pose, seq, cycle, weight, false, depth + 1);
    }

    fn add_layers(&self, pose: &mut LocalPose, seq: usize, cycle: f32, weight: f32, local: bool, depth: usize) {
        if depth >= MAX_LAYER_DEPTH {
            return;
        }
        let Some((seq_at, _)) = seq_anim(self.r, seq, 0) else { return };
        let count = self.r.i32(seq_at + SEQ_NUM_AUTOLAYERS).ok().and_then(|n| usize::try_from(n).ok()).unwrap_or(0);
        if count == 0 {
            return;
        }
        let Ok(rel) = self.r.i32(seq_at + SEQ_AUTOLAYER_INDEX) else { return };
        let Ok(table) = self.r.rel(seq_at, rel, "autolayers") else { return };
        for i in 0..count.min(MAX_LAYERS) {
            let at = table + i * AUTOLAYER_SIZE;
            let (Ok(layer_seq), Ok(flags)) = (self.r.i16(at), self.r.i32(at + 4)) else { continue };
            if (flags & AL_LOCAL != 0) != local {
                continue;
            }
            let read = |o: usize| self.r.f32(at + o).ok().filter(|v| v.is_finite()).unwrap_or(0.0);
            let (start, peak, tail, end) = (read(8), read(12), read(16), read(20));
            let Some((layer_cycle, layer_weight)) = layer_weight(flags, (start, peak, tail, end), cycle, weight) else {
                continue;
            };
            let Ok(layer_seq) = usize::try_from(layer_seq) else { continue };
            if layer_seq == seq {
                continue;
            }
            self.accumulate(pose, layer_seq, layer_cycle, layer_weight, depth + 1);
        }
    }

    fn add_autoplay(&self, pose: &mut LocalPose, seq: usize, time_s: f32) {
        let Some((frames, fps, _)) = anim_timing(self.r, seq, 0) else { return };
        let cycle = if frames > 1 { (time_s * fps / (frames - 1) as f32).rem_euclid(1.0) } else { 0.0 };
        self.accumulate(pose, seq, cycle, 1.0, 1);
    }
}

fn layer_weight(
    flags: i32,
    (start, peak, tail, end): (f32, f32, f32, f32),
    cycle: f32,
    weight: f32,
) -> Option<(f32, f32)> {
    if start == end {
        return Some((cycle, weight));
    }
    let index = if flags & AL_POSE != 0 { 0.0 } else { cycle };
    if index < start || index >= end {
        return None;
    }
    let mut s = 1.0;
    if index < peak && start != peak {
        s = (index - start) / (peak - start);
    } else if index > tail && end != tail {
        s = (end - index) / (end - tail);
    }
    if flags & AL_SPLINE != 0 {
        s = s * s * (3.0 - 2.0 * s);
    }
    let w = if flags & AL_XFADE != 0 && index > tail {
        let d = 1.0 - weight + s * weight;
        if d.abs() > 1e-6 { s * weight / d } else { weight }
    } else if flags & AL_NOBLEND != 0 {
        s
    } else {
        weight * s
    };
    let layer_cycle = if flags & AL_POSE != 0 { cycle } else { (cycle - start) / (end - start) };
    Some((layer_cycle, w.clamp(0.0, 1.0)))
}

fn quat_mul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn quat_scale(p: [f32; 4], t: f32) -> [f32; 4] {
    let sinom = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().min(1.0);
    let sinsom = (sinom.asin() * t).sin();
    let k = sinsom / (sinom + f32::EPSILON);
    let r = (1.0 - sinsom * sinsom).max(0.0).sqrt();
    [p[0] * k, p[1] * k, p[2] * k, if p[3] < 0.0 { -r } else { r }]
}

fn bone_weight(r: &Reader<'_>, seq_at: usize, bone: usize) -> f32 {
    match r.i32(seq_at + SEQ_WEIGHTLIST_INDEX) {
        Ok(0) | Err(_) => 1.0,
        Ok(rel) => r
            .rel(seq_at, rel, "bone weights")
            .ok()
            .and_then(|w| r.f32(w + bone * 4).ok())
            .filter(|w| w.is_finite())
            .unwrap_or(1.0),
    }
}

fn slerp_bones(r: &Reader<'_>, seq: usize, pose: &mut LocalPose, layer: &LocalPose, weight: f32, delta: bool) {
    let weight = weight.clamp(0.0, 1.0);
    if weight <= 0.0 {
        return;
    }
    let Some((seq_at, _)) = seq_anim(r, seq, 0) else { return };
    let flags = r.u32(seq_at + SEQ_FLAGS).unwrap_or(0);
    let delta = delta || flags & STUDIO_DELTA != 0;
    let post = flags & STUDIO_POST != 0;
    for (bone, (slot, add)) in pose.iter_mut().zip(layer).enumerate() {
        let s = weight * bone_weight(r, seq_at, bone);
        if s <= 0.0 {
            continue;
        }
        if delta {
            let scaled = quat_scale(add.1, s);
            let q = if post { quat_mul(slot.1, scaled) } else { quat_mul(scaled, slot.1) };
            slot.1 = quat_blend(q, q, 0.0);
            for c in 0..3 {
                slot.0[c] += add.0[c] * s;
            }
        } else {
            slot.1 = quat_blend(slot.1, add.1, s.min(1.0));
            for c in 0..3 {
                slot.0[c] = slot.0[c] * (1.0 - s) + add.0[c] * s;
            }
        }
    }
}

fn pose_param(r: &Reader<'_>, i: i32) -> Option<String> {
    let count = r.i32(H_NUM_POSE_PARAMS).ok()?;
    if i < 0 || i >= count {
        return None;
    }
    let table = r.rel(0, r.i32(H_NUM_POSE_PARAMS + 4).ok()?, "pose parameters").ok()?;
    let at = table.checked_add(usize::try_from(i).ok()?.checked_mul(POSE_PARAM_SIZE)?)?;
    let name_at = r.rel(at, r.i32(at).ok()?, "pose parameter name").ok()?;
    r.cstr(name_at, "pose parameter name").ok()
}

fn blend_cell(r: &Reader<'_>, seq: usize, params: &[(&str, f32)]) -> Option<usize> {
    let (seq_at, _) = seq_anim(r, seq, 0)?;
    r.check(seq_at, SEQDESC_SIZE, "sequence").ok()?;
    let mut cell = [0usize; 2];
    for (d, slot) in cell.iter_mut().enumerate() {
        let size = usize::try_from(r.i32(seq_at + 68 + d * 4).ok()?).ok()?;
        if size <= 1 {
            continue;
        }
        let param = r.i32(seq_at + 76 + d * 4).ok()?;
        let start = r.f32(seq_at + 84 + d * 4).ok()?;
        let end = r.f32(seq_at + 92 + d * 4).ok()?;
        let name = pose_param(r, param).unwrap_or_default();
        let value = params.iter().find(|(n, _)| n.eq_ignore_ascii_case(&name)).map_or(0.0, |(_, v)| *v);
        let t = if (end - start).abs() > 1e-6 { ((value - start) / (end - start)).clamp(0.0, 1.0) } else { 0.0 };
        *slot = (t * (size - 1) as f32).round() as usize;
    }
    let width = usize::try_from(r.i32(seq_at + 68).ok()?).ok()?.max(1);
    Some(cell[1] * width + cell[0])
}
