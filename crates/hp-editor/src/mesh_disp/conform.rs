use glam::DVec3;
use hp_vmf::DispInfo;

use super::sample::Sampler;
use super::{AxisChoice, Frame, MeshDispError, MeshDispOptions, Origin, Pick, Tri, prepare};
use crate::document::Document;
use crate::selection::FaceRef;
use crate::{EditError, EditResult};

const UNDO_CONFORM: &str = "Conform displacements to mesh";

#[derive(Clone, Debug, PartialEq)]
pub struct ConformOptions {
    pub axis: AxisChoice,
    pub flip: bool,
    pub scale: f64,
    pub origin: Origin,
    pub position: DVec3,
    pub pick: Pick,
    pub strength: f64,
    pub offset: f64,
}

impl Default for ConformOptions {
    fn default() -> Self {
        Self {
            axis: AxisChoice::Z,
            flip: false,
            scale: 1.0,
            origin: Origin::Keep,
            position: DVec3::ZERO,
            pick: Pick::Top,
            strength: 1.0,
            offset: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConformResult {
    pub faces: usize,
    pub vertices: usize,
    pub missed: usize,
    pub overhang: usize,
}

fn face_frame(doc: &Document, faces: &[FaceRef], opts: &ConformOptions) -> Frame {
    let (axis, flip) = match opts.axis {
        AxisChoice::Z => (2, opts.flip),
        AxisChoice::X => (0, opts.flip),
        AxisChoice::Y => (1, opts.flip),
        AxisChoice::Best => {
            let mut abs = DVec3::ZERO;
            let mut signed = DVec3::ZERO;
            for f in faces {
                if let Some(m) = doc.scene.solids.get(&f.solid).and_then(|n| n.disp_mesh(f.side)) {
                    abs += m.face_normal.abs();
                    signed += m.face_normal;
                }
            }
            let axis = if abs.x >= abs.y && abs.x >= abs.z {
                0
            } else if abs.y >= abs.z {
                1
            } else {
                2
            };
            (axis, (signed[axis] < 0.0) ^ opts.flip)
        }
    };
    Frame::new(axis, flip)
}

pub(super) struct Conformer<'a> {
    pub frame: Frame,
    pub sampler: &'a Sampler,
    pub opts: &'a ConformOptions,
}

impl Conformer<'_> {
    pub(super) fn apply(
        &self,
        disp: &mut DispInfo,
        positions: &[DVec3],
        base: &[DVec3],
        face_normal: DVec3,
        stats: &mut ConformResult,
    ) {
        let (frame, sampler, opts) = (&self.frame, self.sampler, self.opts);
        let up = frame.up();
        let elevation = if disp.elevation.is_finite() { face_normal * disp.elevation } else { DVec3::ZERO };
        let mut scratch = Vec::with_capacity(16);
        for (k, bp) in base.iter().enumerate() {
            let l = frame.local(*bp);
            let Some(hit) = sampler.sample(l.x, l.y, opts.pick, &mut scratch) else {
                stats.missed += 1;
                continue;
            };
            if hit.overhang {
                stats.overhang += 1;
            }
            let target = frame.world(DVec3::new(l.x, l.y, hit.height + opts.offset));
            let current = positions.get(k).copied().unwrap_or(*bp);
            let wanted = current.lerp(target, opts.strength);
            let rest = wanted - *bp - elevation;
            let mut n = disp.normals.get(k).copied().unwrap_or(face_normal);
            if n.length_squared() < 1e-12 {
                n = face_normal;
            }
            let (dist, off) = if n.normalize_or_zero().dot(up).abs() >= 0.35 {
                let d = rest.dot(n) / n.length_squared();
                (d, rest - n * d)
            } else {
                (0.0, rest)
            };
            if let Some(slot) = disp.distances.get_mut(k) {
                *slot = dist;
            }
            if let Some(slot) = disp.offsets.get_mut(k) {
                *slot = if off.length_squared() < 1e-18 { DVec3::ZERO } else { off };
            }
            stats.vertices += 1;
        }
    }
}

impl Document {
    pub fn conform_displacements(
        &mut self,
        faces: &[FaceRef],
        world: &[Tri],
        opts: &ConformOptions,
    ) -> EditResult<ConformResult> {
        if !(0.0..=1.0).contains(&opts.strength) || !opts.strength.is_finite() {
            return Err(MeshDispError::Option("strength must be between 0 and 1".to_owned()).into());
        }
        if !opts.offset.is_finite() || opts.offset.abs() > 16384.0 {
            return Err(MeshDispError::Option("offset must be between -16384 and 16384".to_owned()).into());
        }
        let mut targets: Vec<FaceRef> = Vec::new();
        for f in faces {
            let has = self.scene.solids.get(&f.solid).is_some_and(|n| n.disp_mesh(f.side).is_some());
            if has && !targets.contains(f) {
                targets.push(*f);
            }
        }
        if targets.is_empty() {
            return Err(EditError::Nothing("no displacement to conform"));
        }
        let frame = face_frame(self, &targets, opts);
        let mo = MeshDispOptions {
            scale: opts.scale,
            origin: opts.origin,
            position: opts.position,
            pick: opts.pick,
            ..MeshDispOptions::default()
        };
        let prep = prepare(world, &mo, Some(frame))?;
        let sampler = Sampler::new(prep.tris, 8.0);
        let conformer = Conformer { frame, sampler: &sampler, opts };
        let mut stats = ConformResult::default();
        let mut updates = Vec::new();
        for f in &targets {
            let Some(node) = self.scene.solids.get(&f.solid) else { continue };
            let (Some(mesh), Some(disp)) =
                (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_ref()))
            else {
                continue;
            };
            let mut data = (**disp).clone();
            conformer.apply(&mut data, &mesh.positions, &mesh.base_positions, mesh.face_normal, &mut stats);
            if let Some(poly) = node.geom.face_for_side(f.side) {
                hp_geom::update_disp_tags(&mut data, &poly.vertices);
            }
            stats.faces += 1;
            updates.push((*f, data));
        }
        if stats.vertices == 0 {
            return Err(MeshDispError::NoHit.into());
        }
        self.set_disp_data(UNDO_CONFORM, updates)?;
        Ok(stats)
    }
}
