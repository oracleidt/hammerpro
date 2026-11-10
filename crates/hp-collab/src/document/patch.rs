use anyhow::{Result, bail, ensure};

use super::{Change, Snapshot, digest};

const MARKER: &str = "\u{1}hp-patch\n";
const MIN_PATCHED: usize = 160;

pub fn is_patch(text: &str) -> bool {
    text.starts_with(MARKER)
}

pub fn encode_patch(before: &str, after: &str) -> Option<String> {
    if after.len() < MIN_PATCHED {
        return None;
    }
    let old: Vec<&str> = before.split_inclusive('\n').collect();
    let new: Vec<&str> = after.split_inclusive('\n').collect();
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let room = old.len().min(new.len()) - prefix;
    let suffix = old.iter().rev().zip(new.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
    let (old_middle, new_middle) = (&old[prefix..old.len() - suffix], &new[prefix..new.len() - suffix]);
    let mut patch = String::from(MARKER);
    let mut op = |start: usize, remove: usize, insert: &[&str]| {
        patch.push_str(&format!("{start} {remove} {}\n", insert.len()));
        for line in insert {
            patch.push_str(line);
        }
    };
    if old_middle.len() == new_middle.len() {
        let mut index = 0;
        while index < new_middle.len() {
            if old_middle[index] == new_middle[index] {
                index += 1;
                continue;
            }
            let start = index;
            while index < new_middle.len() && old_middle[index] != new_middle[index] {
                index += 1;
            }
            op(prefix + start, index - start, &new_middle[start..index]);
        }
    } else {
        op(prefix, old_middle.len(), new_middle);
    }
    (patch.len() * 4 < after.len() * 3).then_some(patch)
}

pub fn apply_patch(before: &str, patch: &str) -> Result<String> {
    let mut body = patch.strip_prefix(MARKER).ok_or_else(|| anyhow::anyhow!("Invalid map patch"))?;
    let old: Vec<&str> = before.split_inclusive('\n').collect();
    let mut out = String::with_capacity(before.len() + body.len());
    let mut cursor = 0;
    while !body.is_empty() {
        let (header, rest) = body.split_once('\n').ok_or_else(|| anyhow::anyhow!("Invalid map patch"))?;
        let mut numbers = header.split(' ').map(|part| part.parse::<usize>());
        let (Some(Ok(start)), Some(Ok(remove)), Some(Ok(insert)), None) =
            (numbers.next(), numbers.next(), numbers.next(), numbers.next())
        else {
            bail!("Invalid map patch")
        };
        ensure!(start >= cursor && start.checked_add(remove).is_some_and(|end| end <= old.len()), "Invalid map patch");
        for line in &old[cursor..start] {
            out.push_str(line);
        }
        body = rest;
        for _ in 0..insert {
            ensure!(!body.is_empty(), "Invalid map patch");
            let end = body.find('\n').map_or(body.len(), |index| index + 1);
            out.push_str(&body[..end]);
            body = &body[end..];
        }
        cursor = start + remove;
    }
    for line in &old[cursor..] {
        out.push_str(line);
    }
    Ok(out)
}

pub fn patch_changes(changes: &[Change], base: &Snapshot) -> Vec<Change> {
    changes
        .iter()
        .map(|change| {
            let mut change = change.clone();
            if let (Some(before), Some(after)) = (base.0.get(&change.key), change.after.as_mut())
                && change.before.is_some()
                && let Some(patch) = encode_patch(&before.vmf, &after.vmf)
            {
                after.vmf = patch;
            }
            change
        })
        .collect()
}

pub fn expand_changes(changes: &mut [Change], base: &Snapshot) -> Result<()> {
    for change in changes {
        let Some(after) = change.after.as_mut() else { continue };
        if !is_patch(&after.vmf) {
            continue;
        }
        let before = base.0.get(&change.key).ok_or_else(|| anyhow::anyhow!("Invalid map patch"))?;
        ensure!(change.before == Some(digest(before)), "An object changed on another participant: {}", change.key);
        after.vmf = apply_patch(&before.vmf, &after.vmf)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(rows: &[&str]) -> String {
        rows.iter().map(|row| format!("\t\t\"{row}\"\n")).collect::<String>()
    }

    #[test]
    fn patches_roundtrip_and_shrink_row_edits() {
        let rows: Vec<String> = (0..64).map(|index| format!("row{index} 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0")).collect();
        let before = text(&rows.iter().map(String::as_str).collect::<Vec<_>>());
        let mut edited = rows.clone();
        edited[3] = "row3 1 2 3".into();
        edited[40] = "row40 9 9 9".into();
        edited[41] = "row41 9 9 9".into();
        let after = text(&edited.iter().map(String::as_str).collect::<Vec<_>>());
        let patch = encode_patch(&before, &after).unwrap();
        assert!(patch.len() * 10 < after.len());
        assert_eq!(apply_patch(&before, &patch).unwrap(), after);
        let mut longer = rows.clone();
        longer.insert(10, "inserted".into());
        longer.remove(50);
        longer.push("tail".into());
        let after = text(&longer.iter().map(String::as_str).collect::<Vec<_>>());
        if let Some(patch) = encode_patch(&before, &after) {
            assert_eq!(apply_patch(&before, &patch).unwrap(), after);
        }
        let unterminated = format!("{before}last line without newline");
        let patch = encode_patch(&before, &unterminated).unwrap();
        assert_eq!(apply_patch(&before, &patch).unwrap(), unterminated);
        assert!(encode_patch(&before, "short").is_none());
    }

    fn cube(id: u32, x: f64, material: &str) -> crate::document::Unit {
        let mut sides = String::new();
        for (index, plane) in [
            "(0 64 64) (64 64 64) (64 0 64)",
            "(0 0 0) (64 0 0) (64 64 0)",
            "(64 64 64) (0 64 64) (0 64 0)",
            "(64 0 0) (0 0 0) (0 0 64)",
            "(64 64 0) (64 0 0) (64 0 64)",
            "(0 64 64) (0 0 64) (0 0 0)",
        ]
        .iter()
        .enumerate()
        {
            sides.push_str(&format!("side {{\"id\" \"{}\" \"plane\" \"{plane}\" \"material\" \"{material}\" \"uaxis\" \"[1 0 0 0] 0.25\" \"vaxis\" \"[0 -1 0 0] 0.25\"}}", id * 10 + index as u32));
        }
        let mut solid = hp_vmf::parse_fragment(&format!("solid {{\"id\" \"{id}\" {sides}}}")).unwrap().0.remove(0);
        for side in &mut solid.sides {
            for point in &mut side.plane {
                point.x += x;
            }
        }
        crate::document::Unit::from_solid(&solid, 1).unwrap()
    }

    fn wire(changes: Vec<Change>) -> (usize, usize) {
        let message = crate::protocol::Wire::Applied { author: 2, sequence: 1, revision: 1, changes };
        let json = serde_json::to_vec(&message).unwrap();
        let mut compressed = json.len();
        if json.len() >= 4096 {
            use std::io::Write;
            let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(1));
            encoder.write_all(&json).unwrap();
            compressed = compressed.min(encoder.finish().unwrap().len());
        }
        (json.len() + 4, compressed + 4)
    }

    #[test]
    #[ignore = "prints the bytes sent per edit before and after delta encoding and compression"]
    fn wire_sizes_profile() {
        let mut base = Snapshot::default();
        let mut entity = hp_vmf::Entity { id: 2, classname: "prop_static".into(), ..Default::default() };
        for (key, value) in [
            ("origin", "0 0 0"),
            ("angles", "0 90 0"),
            ("model", "models/props_c17/oildrum001.mdl"),
            ("skin", "0"),
            ("solid", "6"),
            ("fademindist", "-1"),
            ("fademaxdist", "0"),
            ("disableshadows", "0"),
            ("rendercolor", "255 255 255"),
        ] {
            entity.set(key, value);
        }
        base.0.insert("entity:2".into(), crate::document::Unit::from_point_entity(&entity).unwrap());
        base.0.insert("solid:3".into(), cube(3, 0.0, "dev/dev_measuregeneric01b"));
        let mut disp = cube(4, 0.0, "nature/blendgrassdirt01").to_solid().unwrap();
        let normal = glam::DVec3::Z;
        disp.sides[0].disp = Some(Box::new(hp_vmf::DispInfo::new_flat(4, disp.sides[0].plane[0], normal)));
        base.0.insert("solid:4".into(), crate::document::Unit::from_solid(&disp, 1).unwrap());
        let mut moved = entity.clone();
        moved.set("origin", "16 0 0");
        let prop = vec![Change {
            key: "entity:2".into(),
            before: Some(digest(&base.0["entity:2"])),
            after: Some(crate::document::Unit::from_point_entity(&moved).unwrap()),
        }];
        let brush = vec![Change {
            key: "solid:3".into(),
            before: Some(digest(&base.0["solid:3"])),
            after: Some(cube(3, 16.0, "dev/dev_measuregeneric01b")),
        }];
        let texture = vec![Change {
            key: "solid:3".into(),
            before: Some(digest(&base.0["solid:3"])),
            after: Some(cube(3, 0.0, "concrete/concretefloor001a")),
        }];
        let mut sculpted = disp.clone();
        if let Some(info) = sculpted.sides[0].disp.as_mut() {
            for row in 6..9 {
                for column in 4..12 {
                    info.distances[row * 17 + column] += 8.0;
                }
            }
        }
        let sculpt = vec![Change {
            key: "solid:4".into(),
            before: Some(digest(&base.0["solid:4"])),
            after: Some(crate::document::Unit::from_solid(&sculpted, 1).unwrap()),
        }];
        let paste: Vec<Change> = (0..500)
            .map(|index| Change {
                key: format!("solid:{}", 1000 + index),
                before: None,
                after: Some(cube(1000 + index, index as f64 * 64.0, "dev/dev_measuregeneric01b")),
            })
            .collect();
        for (label, changes) in [
            ("prop move", prop),
            ("brush move", brush),
            ("texture change", texture),
            ("displacement sculpt", sculpt),
            ("paste of 500 brushes", paste),
        ] {
            let (before, _) = wire(changes.clone());
            let (_, after) = wire(patch_changes(&changes, &base));
            let mut expanded = patch_changes(&changes, &base);
            expand_changes(&mut expanded, &base).unwrap();
            assert_eq!(expanded, changes);
            eprintln!("{label}: {before} bytes before, {after} bytes after");
        }
    }

    #[test]
    fn malformed_patches_are_rejected() {
        let base = "a\nb\nc\n";
        for patch in [
            "x",
            "\u{1}hp-patch\n5 1 0\n",
            "\u{1}hp-patch\n1 9 0\n",
            "\u{1}hp-patch\n0 0 2\nx\n",
            "\u{1}hp-patch\n1 1\n",
        ] {
            assert!(apply_patch(base, patch).is_err(), "{patch:?}");
        }
        assert!(apply_patch(base, "\u{1}hp-patch\n2 0 0\n1 0 0\n").is_err());
    }
}
