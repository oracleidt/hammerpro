use hp_bsp::Bsp;

use crate::hubdata::HubData;

pub const LDR_SLOTS: [usize; 5] = [7, 8, 15, 52, 56];
pub const HDR_SLOTS: [usize; 5] = [58, 53, 54, 51, 55];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LdrSummary {
    pub lumps: Vec<(usize, usize)>,
    pub same_lighting: bool,
}

pub fn apply(base: &Bsp, ldr: &HubData) -> Result<(Bsp, LdrSummary), String> {
    let mut summary = LdrSummary::default();
    let Ok(copy) = Bsp::from_vec(base.bytes().to_vec()) else {
        return Err("the recovered BSP cannot be copied".to_owned());
    };
    let mut edit = copy.edit();
    let hdr_light = base.lump(53).map(|c| c.into_owned()).unwrap_or_default();
    for index in LDR_SLOTS {
        let Some(piece) = ldr.piece(index) else { continue };
        let mut bytes = piece.bytes.clone();
        if index == 7 {
            let reference = base.lump(58).or_else(|_| base.lump(7)).map(|c| c.into_owned()).unwrap_or_default();
            if reference.len() != bytes.len() {
                continue;
            }
            for (rec, old) in bytes.chunks_exact_mut(56).zip(reference.chunks_exact(56)) {
                rec[12..14].copy_from_slice(&old[12..14]);
            }
        }
        if index == 8 {
            summary.same_lighting = !hdr_light.is_empty() && hdr_light == bytes;
        }
        edit.replace_lump_with_version(index, bytes.clone(), u32::from(piece.version)).map_err(|e| e.to_string())?;
        summary.lumps.push((index, bytes.len()));
    }
    let out = edit.to_vec().map_err(|e| e.to_string())?;
    Ok((Bsp::from_vec(out).map_err(|e| e.to_string())?, summary))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hubdata::{Fidelity, HubData};

    fn data(pieces: &[(usize, Vec<u8>)]) -> HubData {
        let mut d = HubData::default();
        for (i, b) in pieces {
            d.push(*i, b.clone(), 1, Fidelity::Exact, "test");
        }
        d
    }

    #[test]
    fn the_second_reading_adds_the_other_lighting_set() {
        let mut faces = vec![0u8; 56 * 2];
        faces[12..14].copy_from_slice(&5i16.to_le_bytes());
        let base =
            Bsp::from_vec(hp_bsp::synthetic_bsp(20, &[(53, vec![1u8; 32]), (58, faces.clone()), (7, faces.clone())]))
                .expect("bsp");
        let mut ldr_faces = vec![0u8; 56 * 2];
        ldr_faces[20..24].copy_from_slice(&64i32.to_le_bytes());
        let ldr =
            data(&[(8, vec![2u8; 32]), (7, ldr_faces), (56, vec![3u8; 28]), (52, vec![0u8; 8]), (15, vec![0u8; 88])]);
        let (out, summary) = apply(&base, &ldr).expect("merged");
        assert_eq!(out.lump(8).expect("ldr lighting").into_owned(), vec![2u8; 32]);
        assert_eq!(out.lump(53).expect("hdr lighting").into_owned(), vec![1u8; 32]);
        assert!(!summary.same_lighting);
        let f = out.lump(7).expect("faces").into_owned();
        assert_eq!(i16::from_le_bytes([f[12], f[13]]), 5, "the displacement index of the first reading is kept");
        assert_eq!(
            i32::from_le_bytes([f[20], f[21], f[22], f[23]]),
            64,
            "the light offsets are those of the second reading"
        );
        assert_eq!(summary.lumps.iter().map(|l| l.0).collect::<Vec<_>>(), vec![7, 8, 15, 52, 56]);
        let same = data(&[(8, vec![1u8; 32])]);
        assert!(apply(&base, &same).expect("merged").1.same_lighting);
        let nothing = apply(&base, &data(&[])).expect("merged");
        assert!(nothing.1.lumps.is_empty());
    }
}
