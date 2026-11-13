use std::hash::Hasher;

use hp_vmf::{DispInfo, Entity, Map, Side, Solid, TexAxis};
use rayon::prelude::*;

use crate::Document;
use crate::compile_export::CompileExport;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Fingerprint {
    pub structural: u64,
    pub detail: u64,
    pub bake: u64,
    pub static_props: u64,
    pub detail_props: u64,
    pub logic: u64,
    pub brush_models: u64,
}

impl Document {
    pub fn build_fingerprint(&self) -> Fingerprint {
        fingerprint_map(&self.compile_snapshot(&CompileExport::default()).map)
    }
}

#[derive(Clone)]
struct H {
    state: u64,
}

impl H {
    fn new(seed: u64) -> Self {
        H { state: seed ^ 0x5AFE_C0DE_1234_5678 }
    }

    #[inline]
    fn word(&mut self, w: u64) {
        let mut z = self.state ^ w;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        self.state = (z ^ (z >> 31)).wrapping_add(0x9E37_79B9_7F4A_7C15);
    }

    fn f(&mut self, v: f64) {
        self.word(if v == 0.0 { 0 } else { v.to_bits() });
    }

    fn v3(&mut self, v: glam::DVec3) {
        self.f(v.x);
        self.f(v.y);
        self.f(v.z);
    }

    fn s(&mut self, s: &str) {
        self.word(s.len() as u64);
        for c in s.as_bytes().chunks(8) {
            let mut b = [0u8; 8];
            b.get_mut(..c.len()).unwrap_or_default().copy_from_slice(c);
            self.word(u64::from_le_bytes(b));
        }
    }

    fn s_ci(&mut self, s: &str) {
        self.word(s.len() as u64);
        for c in s.as_bytes().chunks(8) {
            let mut b = [0u8; 8];
            for (d, x) in b.iter_mut().zip(c) {
                *d = if *x == b'\\' { b'/' } else { x.to_ascii_lowercase() };
            }
            self.word(u64::from_le_bytes(b));
        }
    }

    fn get(&self) -> u64 {
        self.state
    }
}

impl Hasher for H {
    fn finish(&self) -> u64 {
        self.state
    }
    fn write(&mut self, bytes: &[u8]) {
        for c in bytes.chunks(8) {
            let mut b = [0u8; 8];
            b.get_mut(..c.len()).unwrap_or_default().copy_from_slice(c);
            self.word(u64::from_le_bytes(b));
        }
    }
}

fn contents_key(material: &str) -> Option<&str> {
    let lower_starts =
        |p: &str| material.len() >= p.len() && material.get(..p.len()).is_some_and(|m| m.eq_ignore_ascii_case(p));
    let contains_water = material.to_ascii_lowercase().contains("water");
    (lower_starts("tools/") || lower_starts("tools\\") || contains_water).then_some(material)
}

fn axis(h: &mut H, a: &TexAxis) {
    h.v3(a.axis);
    h.f(a.shift);
    h.f(a.scale);
}

fn disp(h: &mut H, d: &DispInfo) {
    h.word(u64::from(d.power));
    h.v3(d.start_position);
    h.word(u64::from(d.flags.unsigned_abs()) ^ u64::from(d.flags < 0));
    h.f(d.elevation);
    h.word(u64::from(d.subdiv));
    for v in d.normals.iter().chain(&d.offsets).chain(&d.offset_normals) {
        h.v3(*v);
    }
    for v in d.distances.iter().chain(&d.alphas) {
        h.f(*v);
    }
    for t in d.triangle_tags.iter().chain(&d.allowed_verts) {
        h.word(u64::from(t.unsigned_abs()) ^ (u64::from(*t < 0) << 63));
    }
}

fn side_detail(h: &mut H, s: &Side) {
    h.s_ci(&s.material);
    axis(h, &s.uaxis);
    axis(h, &s.vaxis);
    h.word(u64::from(s.lightmap_scale.unsigned_abs()));
    h.word(u64::from(s.smoothing_groups));
    if let Some((_, v)) = s.extra_props.iter().find(|(k, _)| k.eq_ignore_ascii_case("hp_detailtype")) {
        h.s_ci(v);
    }
    if let Some(d) = &s.disp {
        disp(h, d);
    }
}

fn plane(h: &mut H, s: &Side) {
    for p in s.plane {
        h.v3(p);
    }
}

fn world_solid(s: &Solid) -> (u64, u64) {
    let mut st = H::new(1);
    let mut de = H::new(2);
    st.word(u64::from(s.id));
    de.word(u64::from(s.id));
    for side in &s.sides {
        plane(&mut st, side);
        if let Some(c) = contents_key(&side.material) {
            st.s_ci(c);
        }
        side_detail(&mut de, side);
    }
    (st.get(), de.get())
}

fn whole_solid(s: &Solid) -> u64 {
    let mut h = H::new(3);
    h.word(u64::from(s.id));
    for side in &s.sides {
        plane(&mut h, side);
        side_detail(&mut h, side);
    }
    h.get()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Bake,
    StaticProps,
    DetailProps,
    Logic,
    Detail,
}

fn part_of(class: &str) -> Part {
    let c = class.to_ascii_lowercase();
    match c.as_str() {
        "prop_static" => Part::StaticProps,
        "prop_detail" => Part::DetailProps,
        "info_overlay" | "info_overlay_transition" => Part::Detail,
        "light_dynamic" => Part::Logic,
        _ if c.starts_with("light") => Part::Bake,
        _ => Part::Logic,
    }
}

fn is_vis_entity(class: &str) -> bool {
    ["func_areaportal", "func_areaportalwindow", "func_viscluster"].iter().any(|c| class.eq_ignore_ascii_case(c))
}

fn is_light_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.starts_with('_') || k.contains("shadow") || k.starts_with("vrad")
}

fn entity_light_keys(e: &Entity) -> u64 {
    let mut h = H::new(7);
    let mut any = false;
    for (k, v) in e.props.iter().filter(|(k, _)| is_light_key(k)) {
        any = true;
        h.s_ci(k);
        h.s(v);
    }
    if !any {
        return 0;
    }
    h.word(u64::from(e.id));
    h.get()
}

fn entity_keys(e: &Entity, skip_light_keys: bool) -> u64 {
    let mut h = H::new(4);
    h.word(u64::from(e.id));
    h.s_ci(&e.classname);
    for (k, v) in &e.props {
        if skip_light_keys && is_light_key(k) {
            continue;
        }
        h.s_ci(k);
        h.s(v);
    }
    for c in &e.connections {
        h.s(&c.output);
        h.s(&c.target);
        h.s(&c.input);
        h.s(&c.param);
        h.f(f64::from(c.delay));
        h.word(u64::from(c.times.unsigned_abs()) ^ (u64::from(c.times < 0) << 63));
    }
    h.get()
}

struct EntityHash {
    part: Part,
    keys: u64,
    light_keys: u64,
    solids: Option<u64>,
    structural: Option<u64>,
    brush_model: Option<u64>,
}

fn entity_hash(e: &Entity) -> EntityHash {
    let part = part_of(&e.classname);
    let split = matches!(part, Part::Logic | Part::Detail);
    let keys = entity_keys(e, split);
    let light_keys = if split { entity_light_keys(e) } else { 0 };
    let visible: Vec<&Solid> = e.solids.iter().filter(|s| !s.hidden).collect();
    let mut solids = H::new(5);
    for s in &visible {
        solids.word(whole_solid(s));
    }
    let is_detail = e.classname.eq_ignore_ascii_case("func_detail");
    let structural = (is_vis_entity(&e.classname) && !visible.is_empty()).then(|| solids.get());
    let brush_model = (!visible.is_empty() && !is_detail).then(|| {
        let mut h = H::new(6);
        h.word(u64::from(e.id));
        h.s_ci(&e.classname);
        h.get()
    });
    let solids = (!visible.is_empty()).then(|| solids.get());
    EntityHash { part, keys, light_keys, solids, structural, brush_model }
}

pub fn fingerprint_map(map: &Map) -> Fingerprint {
    let world: Vec<(u64, u64)> = map.world.solids.par_iter().filter(|s| !s.hidden).map(world_solid).collect();
    let entities: Vec<EntityHash> = map.entities.par_iter().filter(|e| !e.hidden).map(entity_hash).collect();

    let mut structural = H::new(10);
    let mut detail = H::new(11);
    for (s, d) in &world {
        structural.word(*s);
        detail.word(*d);
    }
    let mut bake = H::new(12);
    let mut statics = H::new(13);
    let mut details = H::new(14);
    let mut logic = H::new(15);
    let mut models = H::new(16);
    for (k, v) in &map.world.props {
        let target = if k.eq_ignore_ascii_case("detailmaterial") || k.eq_ignore_ascii_case("detailvbsp") {
            &mut details
        } else {
            &mut logic
        };
        target.s_ci(k);
        target.s(v);
    }
    for e in &entities {
        match e.part {
            Part::Bake => bake.word(e.keys),
            Part::StaticProps => statics.word(e.keys),
            Part::DetailProps => details.word(e.keys),
            Part::Detail => detail.word(e.keys),
            Part::Logic => logic.word(e.keys),
        }
        if e.light_keys != 0 {
            bake.word(e.light_keys);
        }
        if let Some(s) = e.solids {
            detail.word(s);
        }
        if let Some(s) = e.structural {
            structural.word(s);
        }
        if let Some(m) = e.brush_model {
            models.word(m);
        }
    }
    Fingerprint {
        structural: structural.get(),
        detail: detail.get(),
        bake: bake.get(),
        static_props: statics.get(),
        detail_props: details.get(),
        logic: logic.get(),
        brush_models: models.get(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ObjRef;

    const MAP: &str = r#"
world { "id" "1" "classname" "worldspawn" "skyname" "sky_day01_01" "detailmaterial" "detail/detailsprites"
  solid { "id" "2" side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "BRICK/A" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "lightmapscale" "16" } side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "BRICK/A" } side { "id" "3" "plane" "(0 64 64) (0 0 64) (0 0 0)" "material" "BRICK/A" } side { "id" "4" "plane" "(64 64 0) (64 0 0) (64 0 64)" "material" "BRICK/A" } side { "id" "5" "plane" "(64 64 64) (0 64 64) (0 64 0)" "material" "BRICK/A" } side { "id" "6" "plane" "(64 0 0) (0 0 0) (0 0 64)" "material" "BRICK/A" } }
}
entity { "id" "10" "classname" "light" "_light" "255 255 255 200" "origin" "32 32 200" }
entity { "id" "11" "classname" "logic_relay" "targetname" "relay" "origin" "0 0 0" connections { "OnTrigger" "door,Open,,0,-1" } }
entity { "id" "12" "classname" "prop_static" "model" "models/props_c17/oildrum001.mdl" "origin" "0 0 0" }
entity { "id" "13" "classname" "func_detail"
  solid { "id" "20" side { "id" "21" "plane" "(0 64 164) (64 64 164) (64 0 164)" "material" "BRICK/A" } side { "id" "22" "plane" "(0 0 100) (64 0 100) (64 64 100)" "material" "BRICK/A" } side { "id" "23" "plane" "(0 64 164) (0 0 164) (0 0 100)" "material" "BRICK/A" } side { "id" "24" "plane" "(64 64 100) (64 0 100) (64 0 164)" "material" "BRICK/A" } side { "id" "25" "plane" "(64 64 164) (0 64 164) (0 64 100)" "material" "BRICK/A" } side { "id" "26" "plane" "(64 0 100) (0 0 100) (0 0 164)" "material" "BRICK/A" } }
}
"#;

    fn doc() -> Document {
        Document::from_map(hp_vmf::parse(MAP).expect("parse"))
    }

    fn diff(a: &Fingerprint, b: &Fingerprint) -> Vec<&'static str> {
        let mut out = Vec::new();
        for (name, x, y) in [
            ("structural", a.structural, b.structural),
            ("detail", a.detail, b.detail),
            ("bake", a.bake, b.bake),
            ("static_props", a.static_props, b.static_props),
            ("detail_props", a.detail_props, b.detail_props),
            ("logic", a.logic, b.logic),
            ("brush_models", a.brush_models, b.brush_models),
        ] {
            if x != y {
                out.push(name);
            }
        }
        out
    }

    #[test]
    fn each_kind_of_edit_changes_its_part_only() {
        let base = doc().build_fingerprint();
        assert_eq!(base, doc().build_fingerprint(), "stable");

        let mut d = doc();
        d.set_entity_key(&[11], "targetname", "relay2").expect("key");
        assert_eq!(diff(&base, &d.build_fingerprint()), ["logic"]);

        let mut d = doc();
        d.set_entity_key(&[11], "_minlight", "0.5").expect("key");
        assert_eq!(diff(&base, &d.build_fingerprint()), ["bake"]);
        let mut d = doc();
        d.set_entity_key(&[13], "vrad_brush_cast_shadows", "1").expect("key");
        assert_eq!(diff(&base, &d.build_fingerprint()), ["bake"]);

        let mut d = doc();
        d.set_entity_key(&[10], "_light", "255 0 0 200").expect("key");
        assert_eq!(diff(&base, &d.build_fingerprint()), ["bake"]);

        let mut d = doc();
        d.set_entity_key(&[12], "origin", "10 0 0").expect("key");
        assert_eq!(diff(&base, &d.build_fingerprint()), ["static_props"]);

        let mut d = doc();
        d.set_world_key("detailmaterial", "detail/other").expect("key");
        assert_eq!(diff(&base, &d.build_fingerprint()), ["detail_props"]);

        let mut d = doc();
        d.apply_material(&[crate::FaceRef::new(2, 0)], "BRICK/B").expect("material");
        assert_eq!(diff(&base, &d.build_fingerprint()), ["detail"]);

        let mut d = doc();
        d.apply_material(&[crate::FaceRef::new(2, 0)], "TOOLS/TOOLSSKYBOX").expect("material");
        assert_eq!(diff(&base, &d.build_fingerprint()), ["structural", "detail"]);

        let mut d = doc();
        d.translate(&[ObjRef::Solid(2)], glam::DVec3::new(16.0, 0.0, 0.0)).expect("move");
        assert!(diff(&base, &d.build_fingerprint()).contains(&"structural"));

        let mut d = doc();
        d.translate(&[ObjRef::Solid(20)], glam::DVec3::new(0.0, 0.0, 16.0)).expect("move");
        assert_eq!(diff(&base, &d.build_fingerprint()), ["detail"]);

        let mut d = doc();
        let vg = d.create_visgroup("Logic", None, &[ObjRef::Entity(11)]).expect("visgroup");
        d.set_visgroup_visible(vg, false).expect("hide");
        assert_eq!(diff(&base, &d.build_fingerprint()), ["logic"]);
        d.set_visgroup_visible(vg, true).expect("show");
        assert_eq!(d.build_fingerprint(), base);
    }

    #[test]
    fn brush_entities_are_brush_models() {
        let base = doc().build_fingerprint();
        let mut d = doc();
        d.tie_to_entity(&[ObjRef::Solid(2)], "func_wall", &[]).expect("tie");
        let after = d.build_fingerprint();
        let changed = diff(&base, &after);
        assert!(changed.contains(&"brush_models") && changed.contains(&"structural"), "{changed:?}");
        assert!(
            entity_hash(&Entity {
                classname: "func_detail".into(),
                solids: vec![Solid::default()],
                ..Entity::default()
            })
            .brush_model
            .is_none()
        );
        assert_eq!(part_of("LIGHT_SPOT") as u8, Part::Bake as u8);
        assert!(part_of("light_dynamic") == Part::Logic);
        assert!(contents_key("tools\\toolshint").is_some() && contents_key("nature/water_canals").is_some());
        assert!(contents_key("concrete/floor").is_none());
    }

    #[test]
    #[ignore = "benchmark"]
    fn bench_large_map() {
        use std::fmt::Write as _;
        let mut s = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\"\n");
        for i in 0..50_000u32 {
            let (x, y) = (f64::from(i % 250) * 64.0, f64::from(i / 250) * 64.0);
            let _ = write!(s, "solid {{ \"id\" \"{}\"", i + 2);
            for k in 0..6 {
                let _ = write!(
                    s,
                    " side {{ \"id\" \"{}\" \"plane\" \"({x} {y} {k}) ({} {y} 0) ({x} {} 0)\" \"material\" \"BRICK/A\" }}",
                    i * 6 + k + 1,
                    x + 32.0,
                    y + 32.0
                );
            }
            s.push_str(" }\n");
        }
        s.push_str("}\n");
        let map = hp_vmf::parse(&s).expect("parse");
        let t = std::time::Instant::now();
        let fp = fingerprint_map(&map);
        eprintln!("fingerprint of 50000 solids: {:?} ({fp:?})", t.elapsed());
    }
}
