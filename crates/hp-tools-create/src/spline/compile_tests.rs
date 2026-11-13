#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use glam::DVec3;
use hp_editor::Document;

use super::build::{GenEnv, plan};
use super::model::{CableStyle, FenceStyle, Mode, Profile, SplineMeta, SplinePoint};
use super::store::{self, WriteSpec};

struct Materials;

impl hp_bspc::MaterialSource for Materials {
    fn facts(&self, name: &str) -> hp_bspc::MaterialFacts {
        hp_bspc::materials::facts_by_name(name)
    }
}

fn variants() -> Vec<SplineMeta> {
    let points: Vec<SplinePoint> =
        [(0.0, 0.0, 64.0), (512.0, 128.0, 64.0), (1024.0, 0.0, 128.0), (1536.0, 256.0, 128.0)]
            .iter()
            .map(|(x, y, z)| SplinePoint { roll: 10.0, ..SplinePoint::new(DVec3::new(*x, *y, *z)) })
            .collect();
    let base = SplineMeta { points, ..SplineMeta::default() };
    let mut out = Vec::new();
    for profile in Profile::ALL {
        let mut m = SplineMeta { mode: Mode::Sweep, profile: *profile, ..base.clone() };
        if *profile == Profile::Custom {
            m.custom = vec![glam::DVec2::new(-8.0, 0.0), glam::DVec2::new(8.0, 0.0), glam::DVec2::new(0.0, 12.0)];
        }
        out.push(m.clone());
        m.detail = false;
        out.push(m);
    }
    out.push(SplineMeta { mode: Mode::Fence, fence: FenceStyle::Rails, ..base.clone() });
    out.push(SplineMeta {
        mode: Mode::Fence,
        fence: FenceStyle::Model,
        post_model: "models/post.mdl".into(),
        post_every_segment: true,
        ..base.clone()
    });
    out.push(SplineMeta { mode: Mode::Cable, ..base.clone() });
    out.push(SplineMeta { mode: Mode::Cable, cable: CableStyle::Brush, ..base.clone() });
    out.push(SplineMeta { mode: Mode::Props, spacing: 128.0, ..base.clone() });
    out.push(SplineMeta { mode: Mode::Path, closed: true, ..base });
    out
}

#[test]
fn every_mode_compiles_with_hp_bspc() {
    let mut doc = Document::new_empty();
    for (i, mut m) in variants().into_iter().enumerate() {
        for p in &mut m.points {
            p.pos.y += i as f64 * 600.0;
        }
        let p = plan(&m, &GenEnv::default()).unwrap();
        store::write(&mut doc, &WriteSpec { anchor: None, meta: &m, plan: &p, lightmap: 16 }).unwrap();
    }
    doc.create_point_entity("info_player_start", DVec3::new(0.0, -256.0, 64.0), &[]).unwrap();
    let text = doc.to_vmf_string();
    if let Some(root) = std::env::var_os("HP_SPLINE_QA_DIR") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("spline_modes.vmf"), &text).unwrap();
    }
    let map = hp_vmf::parse(&text).unwrap();
    let output =
        hp_bspc::compile(&map, &Materials, &hp_bspc::props::NoModels, &hp_bspc::CompileOptions::default(), &mut |_| {})
            .unwrap();
    assert!(output.bsp.len() > 1024);
    let reopened = Document::from_map(map);
    assert_eq!(store::list(&reopened).len(), variants().len());
}
