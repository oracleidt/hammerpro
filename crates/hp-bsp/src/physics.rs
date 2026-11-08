pub fn model_as_phy(data: &[u8], wanted: u32) -> Option<Vec<u8>> {
    let read =
        |offset: usize| data.get(offset..offset.checked_add(4)?).map(|v| i32::from_le_bytes(v.try_into().unwrap()));
    let mut offset = 0usize;
    while offset.checked_add(16)? <= data.len() {
        let model = read(offset)?;
        if model < 0 {
            return None;
        }
        let bytes = usize::try_from(read(offset + 4)?).ok()?;
        let keys = usize::try_from(read(offset + 8)?).ok()?;
        let solids = u32::try_from(read(offset + 12)?).ok()?;
        if solids > 1024 {
            return None;
        }
        let start = offset.checked_add(16)?;
        let end = start.checked_add(bytes)?;
        let next = end.checked_add(keys)?;
        if next > data.len() {
            return None;
        }
        if model as u32 == wanted {
            let mut phy = Vec::with_capacity(bytes.checked_add(16)?);
            for value in [16u32, 0, solids, 0] {
                phy.extend_from_slice(&value.to_le_bytes());
            }
            phy.extend_from_slice(data.get(start..end)?);
            return Some(phy);
        }
        offset = next;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    const NATIVE: &[u8] =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../hp-pcf/tests/data/precip_physics.lump"));

    #[test]
    fn native_brush_models_have_separate_phy_containers() {
        for model in [1, 2] {
            let phy = model_as_phy(NATIVE, model).unwrap();
            assert_eq!(phy.len(), 460);
            assert_eq!(&phy[0..16], &[16, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
            assert_eq!(&phy[20..24], b"VPHY");
        }
        assert!(model_as_phy(NATIVE, 0).is_none());
        assert!(model_as_phy(NATIVE, 3).is_none());
    }

    #[test]
    fn damaged_record_sizes_do_not_cross_model_boundaries() {
        for length in 0..16 {
            assert!(model_as_phy(&NATIVE[..length], 1).is_none());
        }
        for (field, value) in [(4, -1i32), (8, -1), (12, -1), (12, 1025), (4, i32::MAX)] {
            let mut data = NATIVE.to_vec();
            data[field..field + 4].copy_from_slice(&value.to_le_bytes());
            assert!(model_as_phy(&data, 1).is_none());
        }
    }
}
