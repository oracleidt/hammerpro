use std::sync::Arc;

use super::*;

type RawElement = (String, String, Vec<(String, Attr)>);

#[derive(Default)]
struct Writer {
    table: Vec<String>,
    elements: Vec<RawElement>,
}

enum Attr {
    Str(&'static str),
    Float(f32),
    Int(i32),
    Color([u8; 4]),
    Vec3([f32; 3]),
    Elements(Vec<i32>),
    Element(i32),
}

impl Writer {
    fn idx(&mut self, s: &str) -> u16 {
        match self.table.iter().position(|t| t == s) {
            Some(i) => i as u16,
            None => {
                self.table.push(s.to_owned());
                (self.table.len() - 1) as u16
            }
        }
    }

    fn element(&mut self, kind: &str, name: &str, attrs: Vec<(&str, Attr)>) -> i32 {
        self.elements.push((
            kind.to_owned(),
            name.to_owned(),
            attrs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        ));
        (self.elements.len() - 1) as i32
    }

    fn op(&mut self, function: &'static str, mut attrs: Vec<(&str, Attr)>) -> i32 {
        attrs.insert(0, ("functionName", Attr::Str(function)));
        self.element("DmeParticleOperator", function, attrs)
    }

    fn finish(mut self) -> Vec<u8> {
        let kinds: Vec<String> = self.elements.iter().map(|e| e.0.clone()).collect();
        for k in &kinds {
            self.idx(k);
        }
        let names: Vec<String> = self.elements.iter().flat_map(|e| e.2.iter().map(|a| a.0.clone())).collect();
        for n in &names {
            self.idx(n);
        }
        let mut out = b"<!-- dmx encoding binary 2 format pcf 1 -->\n\0".to_vec();
        out.extend_from_slice(&(self.table.len() as u16).to_le_bytes());
        for s in &self.table {
            out.extend_from_slice(s.as_bytes());
            out.push(0);
        }
        out.extend_from_slice(&(self.elements.len() as i32).to_le_bytes());
        let elements = std::mem::take(&mut self.elements);
        for (kind, name, _) in &elements {
            out.extend_from_slice(&self.idx(kind).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            out.extend_from_slice(&[7; 16]);
        }
        for (_, _, attrs) in &elements {
            out.extend_from_slice(&(attrs.len() as i32).to_le_bytes());
            for (name, a) in attrs {
                out.extend_from_slice(&self.idx(name).to_le_bytes());
                match a {
                    Attr::Str(s) => {
                        out.push(5);
                        out.extend_from_slice(s.as_bytes());
                        out.push(0);
                    }
                    Attr::Float(v) => {
                        out.push(3);
                        out.extend_from_slice(&v.to_le_bytes());
                    }
                    Attr::Int(v) => {
                        out.push(2);
                        out.extend_from_slice(&v.to_le_bytes());
                    }
                    Attr::Color(c) => {
                        out.push(8);
                        out.extend_from_slice(c);
                    }
                    Attr::Vec3(v) => {
                        out.push(10);
                        for c in v {
                            out.extend_from_slice(&c.to_le_bytes());
                        }
                    }
                    Attr::Element(i) => {
                        out.push(1);
                        out.extend_from_slice(&i.to_le_bytes());
                    }
                    Attr::Elements(list) => {
                        out.push(15);
                        out.extend_from_slice(&(list.len() as i32).to_le_bytes());
                        for i in list {
                            out.extend_from_slice(&i.to_le_bytes());
                        }
                    }
                }
            }
        }
        out
    }
}

fn sample() -> Vec<u8> {
    let mut w = Writer::default();
    let root = w.element("DmElement", "untitled", vec![]);
    let emit = w.op("emit_continuously", vec![("emission_rate", Attr::Float(50.0))]);
    let sphere = w.op(
        "Position Within Sphere Random",
        vec![("distance_max", Attr::Float(8.0)), ("speed_min", Attr::Float(10.0)), ("speed_max", Attr::Float(20.0))],
    );
    let life = w.op("Lifetime Random", vec![("lifetime_min", Attr::Float(1.0)), ("lifetime_max", Attr::Float(1.0))]);
    let radius = w.op("Radius Random", vec![("radius_min", Attr::Float(4.0)), ("radius_max", Attr::Float(6.0))]);
    let color = w.op(
        "Color Random",
        vec![("color1", Attr::Color([200, 200, 200, 255])), ("color2", Attr::Color([100, 100, 100, 255]))],
    );
    let movement = w.op("Movement Basic", vec![("gravity", Attr::Vec3([0.0, 0.0, 50.0])), ("drag", Attr::Float(0.0))]);
    let decay = w.op("Lifespan Decay", vec![]);
    let fade = w.op(
        "Alpha Fade Out Random",
        vec![("fade out time min", Attr::Float(0.5)), ("fade out time max", Attr::Float(0.5))],
    );
    let grow =
        w.op("Radius Scale", vec![("radius_start_scale", Attr::Float(1.0)), ("radius_end_scale", Attr::Float(3.0))]);
    let render = w.op("render_animated_sprites", vec![]);
    let mystery = w.op("Some Future Operator", vec![("anything", Attr::Int(3))]);
    let burst = w.op("emit_instantaneously", vec![("num_to_emit", Attr::Int(10))]);
    let spark_life =
        w.op("Lifetime Random", vec![("lifetime_min", Attr::Float(0.2)), ("lifetime_max", Attr::Float(0.3))]);
    let spark_decay = w.op("Lifespan Decay", vec![]);
    let spark_render = w.op("render_animated_sprites", vec![]);
    let spark = w.element(
        "DmeParticleSystemDefinition",
        "sample_spark",
        vec![
            ("material", Attr::Str("effects/spark")),
            ("emitters", Attr::Elements(vec![burst])),
            ("initializers", Attr::Elements(vec![spark_life])),
            ("operators", Attr::Elements(vec![spark_decay])),
            ("renderers", Attr::Elements(vec![spark_render])),
        ],
    );
    let child = w.element(
        "DmeParticleChild",
        "sample_spark",
        vec![("child", Attr::Element(spark)), ("delay", Attr::Float(0.1))],
    );
    let smoke = w.element(
        "DmeParticleSystemDefinition",
        "sample_smoke",
        vec![
            ("material", Attr::Str("particle/smoke1/smoke1")),
            ("max_particles", Attr::Int(40)),
            ("radius", Attr::Float(5.0)),
            ("color", Attr::Color([255, 128, 0, 255])),
            ("emitters", Attr::Elements(vec![emit])),
            ("initializers", Attr::Elements(vec![sphere, life, radius, color])),
            ("operators", Attr::Elements(vec![movement, decay, fade, grow, mystery])),
            ("renderers", Attr::Elements(vec![render])),
            ("children", Attr::Elements(vec![child])),
        ],
    );
    w.elements[root as usize].2.push(("particleSystemDefinitions".to_owned(), Attr::Elements(vec![smoke, spark])));
    w.finish()
}

#[test]
fn definitions_are_read() {
    let systems = parse_file(&sample()).expect("parses");
    assert_eq!(systems.len(), 2);
    let smoke = systems.iter().find(|s| s.name == "sample_smoke").expect("smoke");
    assert_eq!(smoke.material, "particle/smoke1/smoke1");
    assert_eq!(smoke.max_particles, 40);
    assert_eq!(smoke.color, [255, 128, 0, 255]);
    assert_eq!(smoke.emitters.len(), 1);
    assert_eq!(smoke.initializers.len(), 4);
    assert_eq!(smoke.operators.len(), 5);
    assert_eq!(smoke.children, vec![Child { system: "sample_spark".into(), delay: 0.1 }]);
    assert_eq!(smoke.initializers[0].float("distance_max"), Some(8.0));
    assert_eq!(smoke.operators[0].vec3("gravity"), Some([0.0, 0.0, 50.0]));
}

#[test]
fn the_simulation_emits_moves_and_ages() {
    let systems = parse_file(&sample()).expect("parses");
    let plans: Vec<Arc<Plan>> = systems.iter().map(|s| Arc::new(Plan::new(s))).collect();
    let smoke = plans.iter().find(|p| p.name == "sample_smoke").expect("smoke").clone();
    assert_eq!(smoke.unsupported, vec!["Some Future Operator".to_owned()]);
    let mut sim = Sim::new(smoke, 7);
    for _ in 0..30 {
        sim.step(1.0 / 30.0, [100.0, 0.0, 0.0], 1000);
    }
    let n = sim.particles.len();
    assert!(n > 20 && n <= 40, "{n}");
    for p in &sim.particles {
        assert!(p.age < p.life + 1e-3);
        assert!((p.pos[0] - 100.0).abs() < 8.0 + 30.0);
        assert!(p.radius >= 4.0 && p.radius <= 6.0 * 3.0 + 1e-3);
        assert!(p.alpha >= 0.0 && p.alpha <= 1.0);
    }
    let oldest = sim.particles.iter().map(|p| p.age).fold(0.0f32, f32::max);
    assert!(oldest > 0.5);
    for _ in 0..60 {
        sim.step(1.0 / 30.0, [100.0, 0.0, 0.0], 1000);
    }
    assert!(sim.particles.iter().all(|p| p.age <= 1.0 + 1e-3));
}

#[test]
fn effects_play_children_and_loop() {
    let systems = parse_file(&sample()).expect("parses");
    let plans: Vec<Arc<Plan>> = systems.iter().map(|s| Arc::new(Plan::new(s))).collect();
    let lookup = |name: &str| plans.iter().find(|p| p.name.eq_ignore_ascii_case(name)).cloned();
    let mut fx = Effect::new("sample_smoke", &lookup, 3).expect("effect");
    assert_eq!(fx.sims.len(), 2);
    assert!(Effect::new("missing", &lookup, 3).is_none());
    fx.step(0.05, [0.0; 3], 500);
    assert_eq!(fx.sims.iter().find(|(_, s)| s.plan().name == "sample_spark").map(|(_, s)| s.particles.len()), Some(0));
    for _ in 0..4 {
        fx.step(0.05, [0.0; 3], 500);
    }
    assert!(fx.particle_count() > 10);
    let mut burst = Effect::new("sample_spark", &lookup, 1).expect("effect");
    burst.step(0.01, [0.0; 3], 500);
    assert_eq!(burst.particle_count(), 10);
    let mut seen_empty = false;
    let mut replayed = false;
    for _ in 0..200 {
        burst.step(0.02, [0.0; 3], 500);
        if burst.particle_count() == 0 {
            seen_empty = true;
        } else if seen_empty {
            replayed = true;
        }
    }
    assert!(seen_empty && replayed);
    let mut capped = Effect::new("sample_smoke", &lookup, 9).expect("effect");
    for _ in 0..60 {
        capped.step(0.05, [0.0; 3], 5);
    }
    assert!(capped.particle_count() <= 5);
}

#[test]
fn hostile_files_never_panic() {
    assert_eq!(parse_file(b""), Err(PcfError::NotDmx));
    assert_eq!(parse_file(b"<!-- dmx encoding binary 9 format pcf 1 -->\n\0"), Err(PcfError::Unsupported(9)));
    let full = sample();
    for n in 0..full.len() {
        let _ = parse_file(&full[..n]);
    }
    let mut seed = 0x2545_F491u32;
    for _ in 0..500 {
        let mut data = full.clone();
        for _ in 0..4 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let i = 48 + (seed as usize) % (data.len() - 48);
            data[i] = (seed >> 24) as u8;
        }
        if let Ok(systems) = parse_file(&data) {
            for s in &systems {
                let mut sim = Sim::new(Arc::new(Plan::new(s)), 1);
                for _ in 0..10 {
                    sim.step(0.1, [0.0; 3], 100);
                }
                assert!(sim.particles.len() <= 100);
            }
        }
    }
}

#[test]
fn native_children_depth_and_particle_capacity_are_preserved() {
    let defs = parse_file(include_bytes!("../tests/data/capacity_native/hp_capacity117.pcf")).unwrap();
    let plans: Vec<_> = defs.iter().map(|d| Arc::new(Plan::new(d))).collect();
    assert!(plans.iter().all(|p| p.unsupported.is_empty()));
    let lookup = |name: &str| plans.iter().find(|p| p.name == name).cloned();
    for (name, collections, particles) in [("children", 21, 21), ("deep", 7, 2)] {
        let mut effect = Effect::new(&format!("hp_capacity117_{name}"), &lookup, 17).unwrap();
        assert_eq!(effect.sims.len(), collections);
        assert!(!effect.hierarchy_limited);
        for _ in 0..8 {
            effect.step(0.05, [0.0, 0.0, 64.0], 80_000);
        }
        assert_eq!(effect.particle_count(), particles);
    }
    for count in [6000, 10000] {
        let mut effect = Effect::new(&format!("hp_capacity117_count{count}"), &lookup, 17).unwrap();
        effect.step(0.05, [0.0, 0.0, 64.0], 80_000);
        assert_eq!(effect.particle_count(), 5000, "retain the native per-collection cap");
        let high = effect.particles().map(|(_, p)| p.pos[0]).fold(0.0f32, f32::max);
        assert_eq!(high.to_bits(), 4999.0f32.to_bits(), "native count-to-alpha-to-position readback");
    }
}

#[test]
fn child_expansion_preserves_siblings_and_bounds_cycles_and_large_graphs() {
    let leaf = Arc::new(Plan::new(&SystemDef { name: "leaf".into(), ..Default::default() }));
    let root = |count| {
        Arc::new(Plan::new(&SystemDef {
            name: "root".into(),
            children: (0..count).map(|_| Child { system: "leaf".into(), delay: 0.0 }).collect(),
            ..Default::default()
        }))
    };
    for (count, expected, limited) in [(2, 3, false), (1000, sim::MAX_SYSTEMS, true)] {
        let root = root(count);
        let effect =
            Effect::new("root", &|name| Some(if name == "root" { root.clone() } else { leaf.clone() }), 1).unwrap();
        assert_eq!(effect.sims.len(), expected);
        assert_eq!(effect.hierarchy_limited, limited);
    }
    let cycle = Arc::new(Plan::new(&SystemDef {
        name: "cycle".into(),
        children: vec![Child { system: "cycle".into(), delay: 0.0 }],
        ..Default::default()
    }));
    let effect = Effect::new("cycle", &|_| Some(cycle.clone()), 1).unwrap();
    assert_eq!(effect.sims.len(), 1);
    assert!(effect.hierarchy_limited);
    let effect = Effect::new("cycle", &|_| Some(Arc::new((*cycle).clone())), 1).unwrap();
    assert_eq!(effect.sims.len(), 1);
    assert!(effect.hierarchy_limited);
}

#[test]
fn real_particle_files() {
    let dirs = [
        "D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod/particles",
        "C:/Program Files (x86)/Steam/steamapps/common/GarrysMod/garrysmod/particles",
    ];
    let Some(dir) = dirs.iter().map(std::path::Path::new).find(|p| p.is_dir()) else { return };
    let mut functions: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    let mut total = 0;
    for entry in std::fs::read_dir(dir).expect("dir").flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcf")) {
            continue;
        }
        let data = std::fs::read(&path).expect("read");
        let systems = parse_file(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        total += systems.len();
        for s in &systems {
            let plan = Plan::new(s);
            for f in s.emitters.iter().chain(&s.initializers).chain(&s.operators).chain(&s.renderers) {
                *functions.entry(f.function.clone()).or_default() += 1;
            }
            let mut sim = Sim::new(Arc::new(plan), 5);
            for _ in 0..20 {
                sim.step(0.05, [0.0; 3], 300);
            }
        }
    }
    eprintln!("{total} systems; functions: {functions:#?}");
    assert!(total > 0);
}
