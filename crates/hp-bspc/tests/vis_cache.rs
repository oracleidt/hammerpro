mod common;

use common::{Vmf, block};
use glam::DVec3;
use hp_bspc::{CompileOptions, VisLevel};

const OTHER: &str = "dev/dev_measuregeneric01";

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("hp_bspc_vis_cache_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch folder");
    dir
}

fn compile(map: &Vmf, opts: &CompileOptions) -> (Vec<u8>, Vec<String>) {
    let parsed = hp_vmf::parse(&map.text()).expect("the VMF parses");
    let mut log = Vec::new();
    let out = hp_bspc::compile(&parsed, &common::GameMats, &hp_bspc::props::NoModels, opts, &mut |l| {
        log.push(l.to_owned());
    })
    .expect("the map compiles");
    (out.bsp, log)
}

fn visibility(bsp: &[u8]) -> Vec<u8> {
    let file = hp_bsp::Bsp::from_vec(bsp.to_vec()).expect("a BSP");
    file.lump(4).map(std::borrow::Cow::into_owned).unwrap_or_default()
}

fn reused(log: &[String]) -> bool {
    log.iter().any(|l| l.contains("visibility reused"))
}

fn cached_files(dir: &std::path::Path) -> usize {
    std::fs::read_dir(dir)
        .map_or(0, |r| r.flatten().filter(|e| e.path().extension().is_some_and(|x| x == "hpvis")).count())
}

#[test]
fn the_visibility_is_reused_while_the_portals_stay_the_same() {
    let dir = scratch("reuse");
    for vis in [VisLevel::Full, VisLevel::Fast] {
        let cache = hp_bspc::vis::VisCache { dir: dir.join(format!("{vis:?}")), salt: 1 };
        let opts = CompileOptions { vis, vis_cache: Some(cache.clone()), ..Default::default() };
        let plain = CompileOptions { vis, ..Default::default() };
        let map = common::maps::generated(7);

        let (first, log) = compile(&map, &opts);
        assert!(!reused(&log), "{vis:?}: nothing to reuse yet");
        assert_eq!(cached_files(&cache.dir), 1, "{vis:?}: the visibility is kept");
        assert_eq!(first, compile(&map, &plain).0, "{vis:?}: the cache does not change the map");

        let (again, log) = compile(&map, &opts);
        assert!(reused(&log), "{vis:?}: {log:#?}");
        assert_eq!(again, first, "{vis:?}: the same bytes from the kept visibility");
        let stats = log.iter().find(|l| l.starts_with("Vis mode:")).cloned().unwrap_or_default();
        assert!(stats.ends_with("(reused)"), "{stats}");

        let mut detailed = common::maps::generated(7);
        detailed.brush_entity(
            "func_detail",
            &[],
            vec![block(DVec3::new(-300.0, -100.0, 40.0), DVec3::new(-260.0, -60.0, 90.0), OTHER)],
        );
        let (with_detail, log) = compile(&detailed, &opts);
        assert!(reused(&log), "{vis:?}: a detail brush leaves the portals alone");
        let fresh = compile(&detailed, &plain).0;
        assert_eq!(visibility(&with_detail), visibility(&fresh), "{vis:?}: the kept visibility is the computed one");
        assert_eq!(with_detail, fresh, "{vis:?}");

        let mut walled = common::maps::generated(7);
        walled.world.push(block(DVec3::new(-200.0, -510.0, 0.0), DVec3::new(-184.0, 100.0, 380.0), OTHER));
        let (walled_bsp, log) = compile(&walled, &opts);
        assert!(!reused(&log), "{vis:?}: a structural wall changes the portals");
        assert_eq!(walled_bsp, compile(&walled, &plain).0);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_damaged_or_foreign_entry_is_computed_again() {
    let dir = scratch("damaged");
    let map = common::maps::generated(7);
    let cache = hp_bspc::vis::VisCache { dir: dir.clone(), salt: 5 };
    let opts = CompileOptions { vis: VisLevel::Full, vis_cache: Some(cache), ..Default::default() };
    let (first, _) = compile(&map, &opts);
    let entry = std::fs::read_dir(&dir)
        .expect("the cache folder")
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|x| x == "hpvis"))
        .expect("a kept visibility");
    let mut data = std::fs::read(&entry).expect("the entry");
    let last = data.len() - 1;
    data[last] ^= 0x55;
    std::fs::write(&entry, &data).expect("damaged");
    let (again, log) = compile(&map, &opts);
    assert!(!reused(&log), "a damaged entry is not trusted");
    assert_eq!(again, first);
    std::fs::write(&entry, &data[..40]).expect("truncated");
    let (again, log) = compile(&map, &opts);
    assert!(!reused(&log));
    assert_eq!(again, first);

    let other_salt = hp_bspc::vis::VisCache { dir: dir.clone(), salt: 6 };
    let (_, log) =
        compile(&map, &CompileOptions { vis: VisLevel::Full, vis_cache: Some(other_salt), ..Default::default() });
    assert!(!reused(&log), "another compiler build computes its own visibility");
    let radius = CompileOptions { radius_override: Some(300.0), ..opts.clone() };
    let (_, log) = compile(&map, &radius);
    assert!(!reused(&log), "another visibility radius");
    let (_, log) = compile(&map, &radius);
    assert!(reused(&log));
    for k in 0..hp_bspc::vis::cache::MAX_ENTRIES + 3 {
        let r = 200.0 + k as f64;
        compile(&map, &CompileOptions { radius_override: Some(r), ..opts.clone() });
    }
    assert!(cached_files(&dir) <= hp_bspc::vis::cache::MAX_ENTRIES);
    let _ = std::fs::remove_dir_all(&dir);
}
