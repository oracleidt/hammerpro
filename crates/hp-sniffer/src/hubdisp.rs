use rayon::prelude::*;

use crate::engine::MemDisp;
use crate::hub::{Field, parse_hub};
use crate::hubdec::{self, Profile};
use crate::procmem::MemSource;
use crate::world::RenderFace;

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn ptr_at(d: &[u8], at: usize, ptr: usize) -> u64 {
    if ptr == 8 {
        u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
    } else {
        u64::from(u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]]))
    }
}

pub fn parse_displacement<S: MemSource + ?Sized>(src: &S, face: usize, object: u64, p: &Profile) -> Option<MemDisp> {
    let head = src.read_vec(object, p.disp_object);
    if head.len() < p.disp_object {
        return None;
    }
    let power = i32_at(&head, p.disp_power);
    if !(2..=4).contains(&power) {
        return None;
    }
    let start = [f32_at(&head, p.disp_start), f32_at(&head, p.disp_start + 4), f32_at(&head, p.disp_start + 8)];
    if !start.iter().all(|v| v.is_finite() && v.abs() <= 1.0e6) {
        return None;
    }
    let verts = ptr_at(&head, p.disp_verts, p.ptr);
    if verts < 0x10000 {
        return None;
    }
    let row = (1usize << power) + 1;
    let raw = src.read_vec(verts, row * row * p.disp_vert_stride);
    if raw.len() != row * row * p.disp_vert_stride {
        return None;
    }
    let positions: Vec<[f32; 3]> =
        raw.chunks_exact(p.disp_vert_stride).map(|c| [f32_at(c, 0), f32_at(c, 4), f32_at(c, 8)]).collect();
    if !positions.iter().all(|v| v.iter().all(|c| c.is_finite() && c.abs() <= 1.0e6)) {
        return None;
    }
    let allowed = p.disp_allowed.and_then(|a| {
        let mut words = [0u32; 10];
        for (k, w) in words.iter_mut().enumerate() {
            *w = u32::from_le_bytes([head[a + k * 4], head[a + k * 4 + 1], head[a + k * 4 + 2], head[a + k * 4 + 3]]);
        }
        words.iter().any(|w| *w != 0).then_some(words)
    });
    Some(MemDisp { face, start, power: power as u32, positions, allowed, ..MemDisp::default() })
}

pub fn read_displacements<S: MemSource + ?Sized>(src: &S, faces: &[RenderFace], p: &Profile) -> Vec<MemDisp> {
    let mut out: Vec<MemDisp> = faces
        .par_iter()
        .enumerate()
        .filter(|(_, f)| f.disp != 0)
        .filter_map(|(i, f)| parse_displacement(src, i, f.disp, p))
        .collect();
    out.sort_by_key(|d| d.face);
    out
}

pub fn refresh_faces<S: MemSource + ?Sized>(src: &S, base: u64, ptr: usize, faces: &mut [RenderFace]) -> bool {
    let Some(p) = hubdec::profile_for(ptr) else { return false };
    let Some(hub) = parse_hub(src, base, ptr) else { return false };
    if hub.count(Field::Surfaces) != faces.len() {
        return false;
    }
    let Some(now) = hubdec::read_faces(src, &hub, &p) else { return false };
    for (f, n) in faces.iter_mut().zip(&now) {
        f.disp = n.disp_ptr;
        f.has_disp = n.flags & crate::world::FACE_HAS_DISP != 0;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hubdec::{X64, X86};
    use crate::procmem::VecMem;

    fn object(p: &Profile, power: i32, verts_at: u64) -> Vec<u8> {
        let mut o = vec![0u8; p.disp_object];
        o[p.disp_power..p.disp_power + 4].copy_from_slice(&power.to_le_bytes());
        for (k, v) in [10.0f32, 20.0, 30.0].iter().enumerate() {
            o[p.disp_start + k * 4..p.disp_start + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        if p.ptr == 8 {
            o[p.disp_verts..p.disp_verts + 8].copy_from_slice(&verts_at.to_le_bytes());
        } else {
            o[p.disp_verts..p.disp_verts + 4].copy_from_slice(&(verts_at as u32).to_le_bytes());
        }
        let a = p.disp_allowed.expect("allowed");
        o[a..a + 4].copy_from_slice(&0xffff_fffdu32.to_le_bytes());
        o
    }

    #[test]
    fn a_displacement_object_gives_its_start_power_vertices_and_allowed_bits_in_both_layouts() {
        for p in [X64, X86] {
            let mut mem = VecMem::default();
            mem.add(0x10_0000, object(&p, 2, 0x20_0000));
            let mut verts = Vec::new();
            for i in 0..25 {
                let mut v = vec![0u8; p.disp_vert_stride];
                v[0..4].copy_from_slice(&(i as f32).to_le_bytes());
                v[4..8].copy_from_slice(&(2.0 * i as f32).to_le_bytes());
                verts.extend(v);
            }
            mem.add(0x20_0000, verts);
            let d = parse_displacement(&mem, 7, 0x10_0000, &p).expect("displacement");
            assert_eq!((d.face, d.power, d.start), (7, 2, [10.0, 20.0, 30.0]));
            assert_eq!(d.positions.len(), 25);
            assert_eq!(d.positions[3], [3.0, 6.0, 0.0]);
            assert_eq!(d.allowed.map(|a| a[0]), Some(0xffff_fffd));
            let face = |disp| RenderFace {
                plane: 0,
                back: false,
                first: 0,
                count: 4,
                texinfo: 0,
                disp,
                has_disp: disp != 0,
                material_sort: 0,
            };
            let all = read_displacements(&mem, &[face(0), face(0x10_0000), face(0xdead_0000)], &p);
            assert_eq!(all.len(), 1);
            assert_eq!(all[0].face, 1);
            let mut bad = object(&p, 9, 0x20_0000);
            bad[p.disp_start..p.disp_start + 4].copy_from_slice(&f32::NAN.to_le_bytes());
            mem.add(0x30_0000, bad);
            assert!(parse_displacement(&mem, 0, 0x30_0000, &p).is_none());
        }
    }
}
