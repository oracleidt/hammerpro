mod common;

use common::lumps::Lumps;
use common::{Vmf, block, block_sides, room};
use glam::DVec3;

const GLASS: &str = "glass/glasswindowbreak070b";
const NODRAW: &str = "tools/toolsnodraw";

fn window_map() -> Vmf {
    let v = DVec3::new;
    let mut m = Vmf::new();
    m.world = room(v(-256.0, -256.0, 0.0), v(256.0, 256.0, 192.0), "dev/dev_measuregeneric01b");
    m.world.push(block(v(-256.0, -1.0, 0.0), v(-48.0, 1.0, 192.0), "dev/dev_measuregeneric01b"));
    m.point("info_player_start", v(0.0, -128.0, 1.0), &[]);
    m.point("light", v(0.0, -128.0, 128.0), &[("_light", "255 255 255 200")]);
    m.brush_entity(
        "func_breakable_surf",
        &[("fragility", "100"), ("surfacetype", "0"), ("spawnflags", "0"), ("propdata", "0")],
        vec![block_sides(v(-48.0, -1.0, 32.0), v(48.0, 1.0, 104.0), [NODRAW, NODRAW, NODRAW, NODRAW, NODRAW, GLASS])],
    );
    m
}

#[test]
fn breakable_glass_gets_its_corners_and_one_face_in_the_bsp() {
    let map = hp_vmf::parse(&window_map().text()).expect("the VMF parses");
    assert!(map.entities.iter().all(|e| e.get("upperleft").is_none()), "the source map has no corners");
    let doc = hp_editor::Document::from_map(map);
    let saved = doc.to_map();
    let text = hp_vmf::write(&saved);
    let out = hp_bspc::compile(&saved, &common::GameMats, &hp_bspc::props::NoModels, &Default::default(), &mut |_| {})
        .expect("the map compiles");
    if let Some(dir) = std::env::var_os("HP_BREAKABLE_OUT") {
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("output dir");
        std::fs::write(dir.join("hp_breakable_glass.vmf"), &text).expect("vmf");
        std::fs::write(dir.join("hp_breakable_glass.bsp"), &out.bsp).expect("bsp");
    }
    assert!(out.leak.is_none());
    let lumps = Lumps::from_bytes(&out.bsp);
    let ents = lumps.entities();
    let glass = ents
        .iter()
        .find(|e| e.iter().any(|(k, v)| k == "classname" && v == "func_breakable_surf"))
        .expect("the glass entity is in the BSP");
    let key = |k: &str| glass.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
    assert_eq!(key("lowerleft"), Some("-48 -1 32"));
    assert_eq!(key("upperleft"), Some("-48 -1 104"));
    assert_eq!(key("upperright"), Some("48 -1 104"));
    assert_eq!(key("lowerright"), Some("48 -1 32"));
    assert_eq!(key("error"), Some("0"));
    let model: usize = key("model").and_then(|m| m.strip_prefix('*')).and_then(|m| m.parse().ok()).expect("bmodel");
    let models = lumps.models();
    let md = &models[model];
    let faces = lumps.faces();
    let names = lumps.texdata_names();
    let infos = lumps.texinfos();
    let model_faces: Vec<_> = faces[md.first_face as usize..(md.first_face + md.num_faces) as usize].iter().collect();
    let glass_faces: Vec<_> = model_faces
        .iter()
        .filter(|f| {
            let ti = &infos[f.texinfo as usize];
            names[ti.texdata as usize].eq_ignore_ascii_case(GLASS)
        })
        .collect();
    assert_eq!(glass_faces.len(), 1, "one glass face, not split");
    assert_eq!(model_faces.len(), 1, "nodraw faces are dropped like VBSP does");
    let mut pts: Vec<[i32; 3]> = lumps
        .face_points(glass_faces[0])
        .iter()
        .map(|p| [p[0].round() as i32, p[1].round() as i32, p[2].round() as i32])
        .collect();
    pts.sort_unstable();
    assert_eq!(pts, vec![[-48, -1, 32], [-48, -1, 104], [48, -1, 32], [48, -1, 104]]);
}
