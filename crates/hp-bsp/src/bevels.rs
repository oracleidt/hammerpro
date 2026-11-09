use std::collections::HashSet;

use crate::{Bsp, BspEdit, BspError, Result, lump};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BevelRepair {
    pub brushes: usize,
    pub sides: usize,
    pub leaf_references: usize,
}

fn i32_at(data: &[u8], at: usize) -> Result<i32> {
    let bytes = data.get(at..at + 4).ok_or_else(|| BspError::corrupt("collision index out of range"))?;
    Ok(i32::from_le_bytes(bytes.try_into().unwrap()))
}

fn u16_at(data: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([data[at], data[at + 1]]))
}

fn finite_plane(planes: &[u8], number: usize) -> Result<[f64; 4]> {
    let bytes =
        planes.get(number * 20..number * 20 + 20).ok_or_else(|| BspError::corrupt("collision plane out of range"))?;
    let plane = std::array::from_fn(|i| f64::from(f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())));
    if plane.iter().all(|v| v.is_finite()) { Ok(plane) } else { Err(BspError::corrupt("non-finite collision plane")) }
}

fn axial(plane: [f64; 4]) -> Option<(usize, f64)> {
    (0..3).find_map(|axis| {
        ((plane[axis] == 1.0 || plane[axis] == -1.0) && (0..3).all(|other| other == axis || plane[other] == 0.0))
            .then_some((axis, plane[axis]))
    })
}

pub fn repair_axial_bevels(src: Bsp) -> Result<Option<(BspEdit, BevelRepair)>> {
    let brushes = src.lump(18)?;
    let sides = src.lump(19)?;
    let planes = src.lump(1)?;
    if !brushes.len().is_multiple_of(12) || !sides.len().is_multiple_of(8) || !planes.len().is_multiple_of(20) {
        return Err(BspError::corrupt("invalid collision lump size"));
    }
    let mut needs_repair = false;
    for brush in brushes.chunks_exact(12) {
        let first = usize::try_from(i32_at(brush, 0)?).map_err(|_| BspError::corrupt("negative first brush side"))?;
        let count = usize::try_from(i32_at(brush, 4)?).map_err(|_| BspError::corrupt("negative brush side count"))?;
        let end = first
            .checked_add(count)
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(|| BspError::corrupt("brush side overflow"))?;
        let original = sides.get(first * 8..end).ok_or_else(|| BspError::corrupt("brush sides out of range"))?;
        for side in original.chunks_exact(8).skip(6) {
            if u16_at(side, 6) != 0 && axial(finite_plane(&planes, u16_at(side, 0))?).is_some() {
                needs_repair = true;
                break;
            }
        }
        if needs_repair {
            break;
        }
    }
    if !needs_repair {
        return Ok(None);
    }
    let mut fixed = Vec::new();
    let mut rewritten = Vec::with_capacity(sides.len());
    let mut new_brushes = brushes.to_vec();
    let mut report = BevelRepair::default();
    for (index, brush) in brushes.chunks_exact(12).enumerate() {
        let first = usize::try_from(i32_at(brush, 0)?).map_err(|_| BspError::corrupt("negative first brush side"))?;
        let count = usize::try_from(i32_at(brush, 4)?).map_err(|_| BspError::corrupt("negative brush side count"))?;
        let end = first
            .checked_add(count)
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(|| BspError::corrupt("brush side overflow"))?;
        let original = sides.get(first * 8..end).ok_or_else(|| BspError::corrupt("brush sides out of range"))?;
        let start = rewritten.len() / 8;
        let mut removed = 0;
        let mut lo = [f64::NEG_INFINITY; 3];
        let mut hi = [f64::INFINITY; 3];
        for (at, side) in original.chunks_exact(8).enumerate() {
            let plane = finite_plane(&planes, u16_at(side, 0))?;
            let axis = axial(plane);
            if at < 6 {
                if let Some((axis, sign)) = axis {
                    if sign > 0.0 {
                        hi[axis] = hi[axis].min(plane[3]);
                    } else {
                        lo[axis] = lo[axis].max(-plane[3]);
                    }
                }
            } else if u16_at(side, 6) != 0 && axis.is_some() {
                removed += 1;
                continue;
            }
            rewritten.extend_from_slice(side);
        }
        new_brushes[index * 12..index * 12 + 4].copy_from_slice(&(start as i32).to_le_bytes());
        new_brushes[index * 12 + 4..index * 12 + 8]
            .copy_from_slice(&((rewritten.len() / 8 - start) as i32).to_le_bytes());
        if removed > 0 {
            if !(0..3).all(|axis| lo[axis].is_finite() && hi[axis].is_finite() && lo[axis] <= hi[axis]) {
                return Err(BspError::corrupt("invalid axial collision bounds"));
            }
            fixed.push((index, lo, hi));
            report.brushes += 1;
            report.sides += removed;
        }
    }
    if fixed.is_empty() {
        return Ok(None);
    }
    let stride = match src.lump_info(lump::LEAFS)?.version {
        0 => 56,
        1 => 32,
        _ => return Err(BspError::corrupt("unsupported collision leaf format")),
    };
    let mut leaves = src.lump(lump::LEAFS)?.into_owned();
    let refs = src.lump(17)?;
    let nodes = src.lump(5)?;
    let models = src.lump(lump::MODELS)?;
    if !leaves.len().is_multiple_of(stride) || !refs.len().is_multiple_of(2) || !nodes.len().is_multiple_of(32) {
        return Err(BspError::corrupt("invalid collision tree size"));
    }
    let mut world_leaves = HashSet::new();
    let mut visited = HashSet::new();
    let head = i32_at(&models, 36)?;
    let mut stack = vec![head];
    while let Some(node) = stack.pop() {
        if !visited.insert(node) {
            continue;
        }
        if node < 0 {
            let leaf = usize::try_from(-1i64 - i64::from(node)).unwrap();
            if leaf >= leaves.len() / stride {
                return Err(BspError::corrupt("collision leaf out of range"));
            }
            world_leaves.insert(leaf);
        } else {
            let at = node as usize * 32;
            stack.push(i32_at(&nodes, at + 4)?);
            stack.push(i32_at(&nodes, at + 8)?);
        }
    }
    let mut world_brushes = HashSet::new();
    let mut per_leaf = Vec::with_capacity(leaves.len() / stride);
    for (index, leaf) in leaves.chunks_exact(stride).enumerate() {
        let first = u16_at(leaf, 24);
        let count = u16_at(leaf, 26);
        let entries =
            refs.get(first * 2..(first + count) * 2).ok_or_else(|| BspError::corrupt("leaf brushes out of range"))?;
        let values: Vec<_> = entries.chunks_exact(2).map(|row| u16_at(row, 0)).collect();
        if world_leaves.contains(&index) {
            world_brushes.extend(values.iter().copied());
        }
        per_leaf.push(values);
    }
    for (brush, lo, hi) in fixed.into_iter().filter(|(brush, _, _)| world_brushes.contains(brush)) {
        let center: [f64; 3] = std::array::from_fn(|axis| (lo[axis] + hi[axis]) * 0.5);
        let extent: [f64; 3] = std::array::from_fn(|axis| (hi[axis] - lo[axis]) * 0.5);
        stack.push(head);
        visited.clear();
        while let Some(node) = stack.pop() {
            if !visited.insert(node) {
                continue;
            }
            if node < 0 {
                let index = (-1i64 - i64::from(node)) as usize;
                let leaf = &leaves[index * stride..(index + 1) * stride];
                if i32_at(leaf, 0)? & 1 == 0 && !per_leaf[index].contains(&brush) {
                    per_leaf[index].push(brush);
                    report.leaf_references += 1;
                }
            } else {
                let at = node as usize * 32;
                let number =
                    usize::try_from(i32_at(&nodes, at)?).map_err(|_| BspError::corrupt("negative collision plane"))?;
                let plane = finite_plane(&planes, number)?;
                let distance = (0..3).map(|axis| center[axis] * plane[axis]).sum::<f64>() - plane[3];
                let radius = (0..3).map(|axis| extent[axis] * plane[axis].abs()).sum::<f64>() + 0.1;
                if distance + radius >= 0.0 {
                    stack.push(i32_at(&nodes, at + 4)?);
                }
                if distance - radius <= 0.0 {
                    stack.push(i32_at(&nodes, at + 8)?);
                }
            }
        }
    }
    let mut new_refs = Vec::new();
    for (index, values) in per_leaf.into_iter().enumerate() {
        let first =
            u16::try_from(new_refs.len() / 2).map_err(|_| BspError::TooLarge("repaired leaf brushes".into()))?;
        let count = u16::try_from(values.len()).map_err(|_| BspError::TooLarge("repaired leaf brush count".into()))?;
        leaves[index * stride + 24..index * stride + 26].copy_from_slice(&first.to_le_bytes());
        leaves[index * stride + 26..index * stride + 28].copy_from_slice(&count.to_le_bytes());
        for value in values {
            let value = u16::try_from(value).map_err(|_| BspError::TooLarge("repaired brush index".into()))?;
            new_refs.extend_from_slice(&value.to_le_bytes());
        }
    }
    if new_refs.len() / 2 > 65536 {
        return Err(BspError::TooLarge("repaired leaf brushes".into()));
    }
    drop((brushes, sides, planes, refs, nodes, models));
    let mut edit = src.edit();
    edit.replace_lump(18, new_brushes)?;
    edit.replace_lump(19, rewritten)?;
    edit.replace_lump(lump::LEAFS, leaves)?;
    edit.replace_lump(17, new_refs)?;
    Ok(Some((edit, report)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(version: u32) -> Bsp {
        let mut planes = Vec::new();
        for axis in 0i32..3 {
            for sign in [-1.0f32, 1.0] {
                for component in 0..3 {
                    planes.extend_from_slice(&(if axis == component { sign } else { 0.0 }).to_le_bytes());
                }
                planes.extend_from_slice(&1.0f32.to_le_bytes());
                planes.extend_from_slice(&axis.to_le_bytes());
            }
        }
        for value in [0.0f32, 0.0, -1.0, -0.5] {
            planes.extend_from_slice(&value.to_le_bytes());
        }
        planes.extend_from_slice(&2i32.to_le_bytes());
        let mut sides = Vec::new();
        for index in 0..7u16 {
            sides.extend_from_slice(&index.to_le_bytes());
            sides.extend_from_slice(&[0; 4]);
            sides.extend_from_slice(&u16::from(index == 6).to_le_bytes());
        }
        let brushes = [0i32, 7, 1].into_iter().flat_map(i32::to_le_bytes).collect();
        let stride = if version == 0 { 56 } else { 32 };
        let mut leaves = vec![0; stride * 2];
        leaves[26..28].copy_from_slice(&1u16.to_le_bytes());
        leaves[stride + 24..stride + 26].copy_from_slice(&1u16.to_le_bytes());
        let mut nodes = vec![0; 32];
        nodes[0..4].copy_from_slice(&1i32.to_le_bytes());
        nodes[4..8].copy_from_slice(&(-1i32).to_le_bytes());
        nodes[8..12].copy_from_slice(&(-2i32).to_le_bytes());
        let src = Bsp::from_vec(crate::synthetic_bsp(
            20,
            &[
                (1, planes),
                (5, nodes),
                (10, leaves),
                (14, vec![0; 48]),
                (17, vec![0; 2]),
                (18, brushes),
                (19, sides),
                (0, b"{\"classname\" \"worldspawn\"}\0".to_vec()),
            ],
        ))
        .unwrap();
        let mut edit = src.edit();
        let leaves = edit.source().lump(10).unwrap().into_owned();
        edit.replace_lump_with_version(10, leaves, version).unwrap();
        Bsp::from_vec(edit.to_vec().unwrap()).unwrap()
    }

    #[test]
    fn repair_removes_bad_bevels_and_restores_world_leaf_collision_references() {
        for version in [0, 1] {
            let src = fixture(version);
            let entities = src.entity_text().unwrap();
            let (edit, report) = repair_axial_bevels(src).unwrap().unwrap();
            assert_eq!(report, BevelRepair { brushes: 1, sides: 1, leaf_references: 1 });
            let result = Bsp::from_vec(edit.to_vec().unwrap()).unwrap();
            assert_eq!(result.entity_text().unwrap(), entities);
            assert_eq!(result.lump(19).unwrap().len(), 6 * 8);
            assert_eq!(&*result.lump(17).unwrap(), &[0, 0, 0, 0]);
            assert!(repair_axial_bevels(result).unwrap().is_none());
        }
    }

    #[test]
    fn malformed_collision_ranges_fail_before_writing_a_map() {
        let mut edit = fixture(1).edit();
        edit.replace_lump(18, [-1i32, 7, 1].into_iter().flat_map(i32::to_le_bytes).collect()).unwrap();
        assert!(repair_axial_bevels(Bsp::from_vec(edit.to_vec().unwrap()).unwrap()).is_err());
    }

    #[test]
    #[ignore = "requires an explicitly selected native BSP fixture and output path"]
    fn native_compiler_fixture_keeps_all_other_lumps_and_repairs_collision() {
        let input = std::path::PathBuf::from(std::env::var_os("HP_BEVEL_QA_INPUT").unwrap());
        let output = std::path::PathBuf::from(std::env::var_os("HP_BEVEL_QA_OUTPUT").unwrap());
        assert_ne!(std::fs::canonicalize(&input).unwrap(), std::path::absolute(&output).unwrap());
        let before = Bsp::read(&input).unwrap();
        let source = Bsp::read(&input).unwrap();
        let (edit, report) = repair_axial_bevels(source).unwrap().unwrap();
        assert!(report.sides > 0 && report.brushes > 0 && report.leaf_references > 0);
        edit.save(&output).unwrap();
        let result = Bsp::read(&output).unwrap();
        for index in 0..crate::LUMP_COUNT {
            if ![10, 17, 18, 19, 35].contains(&index) {
                assert_eq!(before.lump(index).unwrap(), result.lump(index).unwrap(), "lump {index}");
            }
        }
        assert_eq!(
            before.game_lumps().unwrap().to_bytes(0).unwrap(),
            result.game_lumps().unwrap().to_bytes(0).unwrap()
        );
        assert!(repair_axial_bevels(result).unwrap().is_none());
        eprintln!(
            "Repaired {} bevels in {} brushes and restored {} leaf references",
            report.sides, report.brushes, report.leaf_references
        );
    }
}
