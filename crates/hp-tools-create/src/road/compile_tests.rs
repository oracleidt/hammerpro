use glam::DVec3;
use hp_editor::Document;

use super::{
    apply,
    model::{RoadMeta, RoadPoint},
};
use crate::curve::apply::Target;

struct Materials;

impl hp_bspc::MaterialSource for Materials {
    fn facts(&self, name: &str) -> hp_bspc::MaterialFacts {
        hp_bspc::materials::facts_by_name(name)
    }
}

fn word(bytes: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn lump(bytes: &[u8], index: usize) -> &[u8] {
    let at = word(bytes, 8 + index * 16) as usize;
    let len = word(bytes, 12 + index * 16) as usize;
    &bytes[at..at + len]
}

fn largest_gap(bytes: &[u8], start: DVec3, direction: DVec3, length: f64) -> f64 {
    let planes: Vec<_> = lump(bytes, 1)
        .chunks_exact(20)
        .map(|row| {
            let value = |at| f64::from(f32::from_le_bytes(row[at..at + 4].try_into().unwrap()));
            (DVec3::new(value(0), value(4), value(8)), value(12), word(row, 16))
        })
        .collect();
    let sides = lump(bytes, 19);
    let mut intervals = Vec::new();
    for brush in lump(bytes, 18).chunks_exact(12) {
        if word(brush, 8) & 1 == 0 {
            continue;
        }
        let (first, count) = (word(brush, 0) as usize, word(brush, 4) as usize);
        let (mut low, mut high) = (1.0, length - 1.0);
        for (side_index, side) in sides[first * 8..(first + count) * 8].chunks_exact(8).enumerate() {
            let index = u16::from_le_bytes([side[0], side[1]]) as usize;
            let (normal, distance, kind) = planes[index];
            if i16::from_le_bytes([side[6], side[7]]) != 0 {
                assert!(side_index < 6 || kind > 2, "invalid snapped axial edge bevel in compiled road");
                continue;
            }
            let rate = normal.dot(direction);
            let delta = distance - normal.dot(start);
            if rate.abs() < 1e-12 {
                if delta < -1e-5 {
                    low = length;
                    high = 0.0;
                    break;
                }
            } else if rate > 0.0 {
                high = high.min(delta / rate);
            } else {
                low = low.max(delta / rate);
            }
        }
        if high >= low {
            intervals.push((low, high));
        }
    }
    intervals.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    let mut end = 1.0f64;
    let mut gap = 0.0f64;
    for (low, high) in intervals {
        gap = gap.max(low - end);
        end = end.max(high);
    }
    gap.max(length - 1.0 - end)
}

#[test]
fn compiled_bsp_has_continuous_road_collision_across_marking_and_segment_boundaries() {
    for angle in [0.0001f64, 0.001, 0.003, 0.005, 0.31] {
        for optimize in [false, true] {
            let direction = DVec3::new(angle.cos(), angle.sin(), 0.0);
            let start = DVec3::new(12000.0, 12000.0, 64.0);
            let end = start + direction * 1024.0;
            let meta = RoadMeta {
                points: vec![RoadPoint::new(start.x, start.y, start.z), RoadPoint::new(end.x, end.y, end.z)],
                optimize,
                ..RoadMeta::default()
            };
            let mut doc = Document::new_empty();
            apply::create_road(&mut doc, &meta, 16, Target::World).unwrap();
            let map = hp_vmf::parse(&doc.to_vmf_string()).unwrap();
            let output = hp_bspc::compile(
                &map,
                &Materials,
                &hp_bspc::props::NoModels,
                &hp_bspc::CompileOptions::default(),
                &mut |_| {},
            )
            .unwrap();
            for lateral in [-124.0, -64.0, -2.0, 0.0, 2.0, 64.0, 124.0] {
                let origin = start + DVec3::new(-direction.y, direction.x, 0.0) * lateral - DVec3::Z * 4.0;
                let gap = largest_gap(&output.bsp, origin, direction, 1024.0);
                assert!(
                    gap < 0.03125,
                    "compiled road gap {gap} units, angle {angle}, lateral {lateral}, optimize {optimize}"
                );
            }
            if let Some(root) = std::env::var_os("HP_ROAD_QA_DIR") {
                let root = std::path::PathBuf::from(root);
                std::fs::create_dir_all(&root).unwrap();
                let name = format!("road-{angle}-{optimize}");
                std::fs::write(root.join(format!("{name}.vmf")), doc.to_vmf_string()).unwrap();
                std::fs::write(root.join(format!("{name}.bsp")), output.bsp).unwrap();
            }
        }
    }
}

#[test]
fn compiled_curved_and_elevated_road_keeps_its_joints_solid() {
    for optimize in [false, true] {
        let meta = RoadMeta {
            points: vec![
                RoadPoint::new(12000.0, 12000.0, 64.0),
                RoadPoint::new(13000.0, 12020.0, 64.0),
                RoadPoint::new(14000.0, 12300.0, 72.0),
                RoadPoint::new(15000.0, 12400.0, 80.0),
            ],
            optimize,
            ..RoadMeta::default()
        };
        let road = super::build(&meta, &super::RoadEnv::default()).unwrap();
        let mut doc = Document::new_empty();
        apply::create_road(&mut doc, &meta, 16, Target::World).unwrap();
        let output = hp_bspc::compile(
            &hp_vmf::parse(&doc.to_vmf_string()).unwrap(),
            &Materials,
            &hp_bspc::props::NoModels,
            &hp_bspc::CompileOptions::default(),
            &mut |_| {},
        )
        .unwrap();
        let mut runs = Vec::new();
        if let Some(root) = std::env::var_os("HP_ROAD_QA_DIR") {
            let root = std::path::PathBuf::from(root);
            std::fs::create_dir_all(&root).unwrap();
            std::fs::write(root.join(format!("road-curved-{optimize}.vmf")), doc.to_vmf_string()).unwrap();
        }
        for (index, group) in road.samples.windows(3).enumerate() {
            for lateral in [-124.0, -64.0, -2.0, 0.0, 2.0, 64.0, 124.0] {
                let point = |sample: &super::RoadSample| {
                    sample.center + (sample.left_edge - sample.right_edge).normalize() * lateral - DVec3::Z * 4.0
                };
                let start = (point(&group[0]) + point(&group[1])) * 0.5;
                let end = (point(&group[1]) + point(&group[2])) * 0.5;
                let delta = end - start;
                let gap = largest_gap(&output.bsp, start, delta.normalize(), delta.length());
                assert!(
                    gap < 0.03125,
                    "curved road gap {gap}, joint {index}, lateral {lateral}, optimize {optimize}, start {start:?}, end {end:?}"
                );
                runs.push([start.to_array(), end.to_array()]);
            }
        }
        if let Some(root) = std::env::var_os("HP_ROAD_QA_DIR") {
            let root = std::path::PathBuf::from(root);
            std::fs::create_dir_all(&root).unwrap();
            let name = format!("road-curved-{optimize}");
            std::fs::write(root.join(format!("{name}.vmf")), doc.to_vmf_string()).unwrap();
            std::fs::write(root.join(format!("{name}-runs.json")), format!("{runs:?}")).unwrap();
        }
    }
}
