use std::sync::Arc;

use crate::dmx::{Element, Value};
use crate::sheet::{FULL_UV, Sheet, parse_sheet, sheet_from_vtf};
use crate::sim::{Points, RenderKind};
use crate::{Child, Effect, Operator, Plan, Sim, SystemDef};

fn op(function: &str, attrs: Vec<(&str, Value)>) -> Operator {
    Operator {
        function: function.to_owned(),
        params: Element {
            kind: "DmeParticleOperator".into(),
            name: function.to_owned(),
            attrs: attrs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
        },
    }
}

fn fl(v: f32) -> Value {
    Value::Float(v)
}

fn system(name: &str, emitters: Vec<Operator>, inits: Vec<Operator>, ops: Vec<Operator>) -> SystemDef {
    SystemDef {
        name: name.to_owned(),
        material: "particle/test".to_owned(),
        emitters,
        initializers: inits,
        operators: ops,
        renderers: vec![op("render_animated_sprites", vec![])],
        ..Default::default()
    }
}

fn burst(n: i32) -> Operator {
    op("emit_instantaneously", vec![("num_to_emit", Value::Int(n))])
}

fn run(def: &SystemDef, points: &Points, seconds: f32) -> Sim {
    let mut sim = Sim::new(Arc::new(Plan::new(def)), 11);
    let steps = (seconds / 0.05).round() as usize;
    for _ in 0..steps.max(1) {
        sim.step_at(0.05, points, &[], 500);
    }
    sim
}

#[test]
fn local_speeds_turn_with_the_angles() {
    let def = system(
        "s",
        vec![burst(20)],
        vec![op(
            "Position Within Sphere Random",
            vec![
                ("speed_in_local_coordinate_system_min", Value::Vec3([100.0, 0.0, 0.0])),
                ("speed_in_local_coordinate_system_max", Value::Vec3([100.0, 0.0, 0.0])),
            ],
        )],
        vec![op("Movement Basic", vec![])],
    );
    let sim = run(&def, &Points::with_angles([0.0; 3], [0.0, 90.0, 0.0]), 0.5);
    assert_eq!(sim.particles.len(), 20);
    for p in &sim.particles {
        assert!(p.vel[1] > 99.0 && p.vel[0].abs() < 1e-3, "{:?}", p.vel);
        assert!(p.pos[1] > 30.0);
    }
    let down = Points::with_angles([0.0; 3], [90.0, 0.0, 0.0]);
    assert!((down.axes[0][2] + 1.0).abs() < 1e-5);
    assert!((Points::at([0.0; 3]).axes[1][1] - 1.0).abs() < 1e-5);
}

#[test]
fn rotation_spin_sequence_and_flip() {
    let def = system(
        "s",
        vec![burst(50)],
        vec![
            op("Rotation Random", vec![("rotation_offset_min", fl(10.0)), ("rotation_offset_max", fl(20.0))]),
            op("Sequence Random", vec![("sequence_min", Value::Int(2)), ("sequence_max", Value::Int(4))]),
            op("Rotation Yaw Flip Random", vec![("Flip Percentage", fl(0.5))]),
        ],
        vec![op("Rotation Spin Roll", vec![("spin_rate_degrees", fl(90.0))])],
    );
    let sim = run(&def, &Points::at([0.0; 3]), 1.0);
    let mut flipped = 0;
    for p in &sim.particles {
        let deg = p.rotation.to_degrees();
        assert!(deg > 95.0 && deg < 115.0, "{deg}");
        assert!((2..=4).contains(&p.sequence));
        flipped += usize::from(p.flip);
    }
    assert!(flipped > 5 && flipped < 45, "{flipped}");
}

#[test]
fn radius_scale_windows_keep_their_last_value() {
    let def = system(
        "s",
        vec![burst(1)],
        vec![op("Lifetime Random", vec![("lifetime_min", fl(10.0)), ("lifetime_max", fl(10.0))])],
        vec![
            op(
                "Radius Scale",
                vec![
                    ("start_time", fl(0.0)),
                    ("end_time", fl(0.2)),
                    ("radius_start_scale", fl(1.0)),
                    ("radius_end_scale", fl(2.0)),
                ],
            ),
            op(
                "Radius Scale",
                vec![
                    ("start_time", fl(0.5)),
                    ("end_time", fl(1.0)),
                    ("radius_start_scale", fl(2.0)),
                    ("radius_end_scale", fl(4.0)),
                ],
            ),
        ],
    );
    let sim = run(&def, &Points::at([0.0; 3]), 3.3);
    assert!((sim.particles[0].radius - 10.0).abs() < 0.3, "{}", sim.particles[0].radius);
    let sim = run(&def, &Points::at([0.0; 3]), 7.5);
    assert!((sim.particles[0].radius - 15.0).abs() < 0.5, "{}", sim.particles[0].radius);
}

#[test]
fn fades_oscillations_and_ramps() {
    let def = system(
        "s",
        vec![burst(10)],
        vec![op("Lifetime Random", vec![("lifetime_min", fl(2.0)), ("lifetime_max", fl(2.0))])],
        vec![
            op("Alpha Fade In Simple", vec![("proportional fade in time", fl(0.5))]),
            op("Alpha Fade Out Simple", vec![("proportional fade out time", fl(0.25))]),
            op(
                "Oscillate Scalar",
                vec![
                    ("oscillation field", Value::Int(3)),
                    ("oscillation rate min", fl(20.0)),
                    ("oscillation rate max", fl(20.0)),
                    ("oscillation frequency min", fl(0.25)),
                    ("oscillation frequency max", fl(0.25)),
                ],
            ),
            op(
                "Ramp Scalar Linear Simple",
                vec![("ramp field", Value::Int(4)), ("ramp rate", fl(2.0)), ("end time", fl(1.0))],
            ),
            op("Lifespan Decay", vec![]),
        ],
    );
    let sim = run(&def, &Points::at([0.0; 3]), 0.25);
    let p = sim.particles[0];
    assert!((p.alpha - 0.25).abs() < 0.06, "{}", p.alpha);
    let sim = run(&def, &Points::at([0.0; 3]), 1.0);
    let p = sim.particles[0];
    assert!((p.alpha - 1.0).abs() < 1e-3);
    assert!((p.radius - 5.0).abs() > 0.5, "the radius oscillates: {}", p.radius);
    assert!((p.rotation - 2.0).abs() < 0.15, "ramped: {}", p.rotation);
    let sim = run(&def, &Points::at([0.0; 3]), 1.9);
    assert!(sim.particles[0].alpha < 0.3);
    let sim = run(&def, &Points::at([0.0; 3]), 2.2);
    assert!(sim.particles.is_empty());
}

#[test]
fn paths_rings_constraints_and_control_points() {
    let points = Points { pos: vec![[0.0; 3], [200.0, 0.0, 0.0]], ..Points::at([0.0; 3]) };
    let path = system(
        "s",
        vec![burst(40)],
        vec![op(
            "Position Along Path Sequential",
            vec![
                ("start control point number", Value::Int(0)),
                ("end control point number", Value::Int(1)),
                ("particles to map from start to end", Value::Int(40)),
            ],
        )],
        vec![],
    );
    let sim = run(&path, &points, 0.05);
    let xs: Vec<f32> = sim.particles.iter().map(|p| p.pos[0]).collect();
    assert!(xs.iter().all(|x| (0.0..=200.0).contains(x)));
    assert!(xs.windows(2).all(|w| w[1] > w[0]), "start to end in order");
    let ring = system(
        "s",
        vec![burst(30)],
        vec![op("Position Along Ring", vec![("initial radius", fl(50.0))])],
        vec![op("Constrain distance to control point", vec![("maximum distance", fl(30.0))])],
    );
    let sim = run(&ring, &Points::at([0.0; 3]), 0.05);
    for p in &sim.particles {
        let d = (p.pos[0] * p.pos[0] + p.pos[1] * p.pos[1] + p.pos[2] * p.pos[2]).sqrt();
        assert!((d - 30.0).abs() < 0.1, "constrained to 30: {d}");
        assert!(p.pos[2].abs() < 1e-3);
    }
    assert_eq!(points.get(7), [0.0; 3]);
    assert_eq!(points.get(-1), [0.0; 3]);
}

#[test]
fn children_start_on_the_particles_of_their_parent() {
    let parent = SystemDef {
        children: vec![Child { system: "child".into(), delay: 0.0 }],
        ..system(
            "parent",
            vec![burst(5)],
            vec![op(
                "Position Within Box Random",
                vec![("min", Value::Vec3([500.0, 0.0, 0.0])), ("max", Value::Vec3([500.0, 0.0, 0.0]))],
            )],
            vec![],
        )
    };
    let child = system(
        "child",
        vec![op("emit_continuously", vec![("emission_rate", fl(100.0))])],
        vec![op("Position From Parent Particles", vec![])],
        vec![],
    );
    let plans = [Arc::new(Plan::new(&parent)), Arc::new(Plan::new(&child))];
    let lookup = |n: &str| plans.iter().find(|p| p.name.eq_ignore_ascii_case(n)).cloned();
    let mut fx = Effect::new("parent", &lookup, 1).expect("effect");
    assert!(fx.missing.is_empty());
    for _ in 0..10 {
        fx.step_at(0.05, &Points::at([0.0; 3]), 500);
    }
    let kids = &fx.sims[1].1.particles;
    assert!(!kids.is_empty());
    assert!(kids.iter().all(|p| (p.pos[0] - 500.0).abs() < 1e-3));
    let lonely = SystemDef {
        children: vec![Child { system: "Nobody".into(), delay: 0.0 }],
        ..system("x", vec![burst(1)], vec![], vec![])
    };
    let plan = Arc::new(Plan::new(&lonely));
    let only = |n: &str| (n == "x").then(|| plan.clone());
    assert_eq!(Effect::new("x", &only, 1).expect("fx").missing, vec!["Nobody".to_owned()]);
}

#[test]
fn renderers_lifetimes_from_sequences_and_animation_cycles() {
    let mut trail = system("t", vec![burst(1)], vec![], vec![]);
    trail.renderers = vec![op("render_sprite_trail", vec![("max length", fl(64.0)), ("min length", fl(2.0))])];
    let plan = Plan::new(&trail);
    assert_eq!(plan.render.kind, RenderKind::Trail);
    assert_eq!((plan.render.min_len, plan.render.max_len), (2.0, 64.0));
    let mut rope = system("r", vec![burst(1)], vec![], vec![]);
    rope.renderers = vec![op("render_rope", vec![])];
    assert_eq!(Plan::new(&rope).render.kind, RenderKind::Rope);
    let mut models = system("m", vec![burst(1)], vec![], vec![]);
    models.renderers = vec![op("render_models", vec![])];
    let plan = Plan::new(&models);
    assert!(!plan.draws);
    assert_eq!(plan.unsupported, vec!["render_models".to_owned()]);
    let seq = system(
        "q",
        vec![burst(4)],
        vec![
            op("Sequence Random", vec![("sequence_min", Value::Int(1)), ("sequence_max", Value::Int(1))]),
            op("lifetime from sequence", vec![("Frames Per Second", fl(10.0))]),
        ],
        vec![],
    );
    let mut sim = Sim::new(Arc::new(Plan::new(&seq)), 1);
    sim.set_sequence_frames(vec![4, 25]);
    sim.step(0.01, [0.0; 3], 10);
    assert!(sim.particles.iter().all(|p| (p.life - 2.5).abs() < 1e-4));
    let mut fps = system("f", vec![burst(1)], vec![], vec![]);
    fps.renderers = vec![op(
        "render_animated_sprites",
        vec![("animation rate", fl(20.0)), ("use animation rate as FPS", Value::Bool(true))],
    )];
    let plan = Plan::new(&fps);
    let mut p = sim.particles[0];
    p.age = 1.0;
    assert!((plan.cycle(&p, 40.0) - 0.5).abs() < 1e-5);
    let mut fit = system("g", vec![burst(1)], vec![], vec![]);
    fit.renderers = vec![op("render_animated_sprites", vec![("animation_fit_lifetime", Value::Bool(true))])];
    assert!((Plan::new(&fit).cycle(&p, 40.0) - 0.4).abs() < 1e-5, "age 1 of a 2.5 s life");
    assert!((Plan::new(&seq).cycle(&p, 40.0) - 0.1).abs() < 1e-5, "default rate 0.1");
}

#[test]
fn noise_emitters_remaps_and_warm_up() {
    let def = system(
        "n",
        vec![op("emit noise", vec![("emission minimum", fl(10.0)), ("emission maximum", fl(30.0))])],
        vec![
            op("Lifetime Random", vec![("lifetime_min", fl(100.0)), ("lifetime_max", fl(100.0))]),
            op(
                "remap initial scalar",
                vec![
                    ("input field", Value::Int(1)),
                    ("input minimum", fl(0.0)),
                    ("input maximum", fl(200.0)),
                    ("output field", Value::Int(3)),
                    ("output minimum", fl(0.0)),
                    ("output maximum", fl(40.0)),
                ],
            ),
            op(
                "Remap Noise to Scalar",
                vec![("output field", Value::Int(7)), ("output minimum", fl(0.2)), ("output maximum", fl(0.4))],
            ),
        ],
        vec![],
    );
    let sim = run(&def, &Points::at([0.0; 3]), 2.0);
    let n = sim.particles.len();
    assert!((20..=60).contains(&n), "between 10 and 30 a second: {n}");
    for p in &sim.particles {
        assert!((p.radius - 20.0).abs() < 1e-3, "life 100 maps to radius 20: {}", p.radius);
        assert!(p.alpha >= 0.2 - 1e-4 && p.alpha <= 0.4 + 1e-4);
    }
    let fx = Effect::new("n", &|_| Some(Arc::new(Plan::new(&def))), 1).expect("fx");
    assert!(fx.continuous());
    let once =
        Effect::new("b", &|_| Some(Arc::new(Plan::new(&system("b", vec![burst(2)], vec![], vec![])))), 1).expect("fx");
    assert!(!once.continuous());
}

type RawSequence<'a> = (u32, bool, &'a [(f32, [f32; 4])]);

fn sheet_bytes(version: u32, sequences: &[RawSequence<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&version.to_le_bytes());
    out.extend_from_slice(&(sequences.len() as u32).to_le_bytes());
    for (number, clamp, frames) in sequences {
        out.extend_from_slice(&number.to_le_bytes());
        out.extend_from_slice(&u32::from(*clamp).to_le_bytes());
        out.extend_from_slice(&(frames.len() as u32).to_le_bytes());
        out.extend_from_slice(&(frames.len() as f32).to_le_bytes());
        for (duration, uv) in *frames {
            out.extend_from_slice(&duration.to_le_bytes());
            for _ in 0..if version > 0 { 4 } else { 1 } {
                for c in uv {
                    out.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
    }
    out
}

#[test]
fn sprite_sheet_frames() {
    let a = [0.0, 0.0, 0.5, 0.5];
    let b = [0.5, 0.0, 1.0, 0.5];
    let c = [0.0, 0.5, 0.5, 1.0];
    let bytes = sheet_bytes(0, &[(0, false, &[(1.0, a), (1.0, b)]), (3, true, &[(1.0, a), (3.0, c)])]);
    let sheet = parse_sheet(&bytes).expect("sheet");
    assert_eq!(sheet.sequences.len(), 2);
    assert_eq!(sheet.frame_uv(0, 0.25), a);
    assert_eq!(sheet.frame_uv(0, 0.75), b);
    assert_eq!(sheet.frame_uv(0, 1.25), a);
    assert_eq!(sheet.frame_uv(3, 0.2), a);
    assert_eq!(sheet.frame_uv(3, 0.3), c, "durations weigh the frames");
    assert_eq!(sheet.frame_uv(3, 7.0), c);
    assert_eq!(sheet.frame_count(5), 2);
    assert_eq!(sheet.frame_uv(0, f32::NAN), a);
    assert_eq!(Sheet::default().frame_uv(0, 0.5), FULL_UV);
    let v1 = parse_sheet(&sheet_bytes(1, &[(0, false, &[(1.0, b)])])).expect("v1");
    assert_eq!(v1.frame_uv(0, 0.5), b);
    for n in 0..bytes.len() {
        let _ = parse_sheet(&bytes[..n]);
    }
    let mut huge = bytes.clone();
    huge[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(parse_sheet(&huge).is_none());
    let mut vtf = vec![0u8; 88];
    vtf[..4].copy_from_slice(b"VTF\0");
    vtf[4..8].copy_from_slice(&7u32.to_le_bytes());
    vtf[8..12].copy_from_slice(&3u32.to_le_bytes());
    vtf[68..72].copy_from_slice(&1u32.to_le_bytes());
    vtf[80..84].copy_from_slice(&[0x10, 0, 0, 0]);
    vtf[84..88].copy_from_slice(&88u32.to_le_bytes());
    vtf.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    vtf.extend_from_slice(&bytes);
    assert_eq!(sheet_from_vtf(&vtf), Some(sheet));
    for n in 0..vtf.len() {
        let _ = sheet_from_vtf(&vtf[..n]);
    }
    vtf[8..12].copy_from_slice(&2u32.to_le_bytes());
    assert!(sheet_from_vtf(&vtf).is_none());
    assert!(sheet_from_vtf(b"nope").is_none());
}

#[test]
fn real_sprite_sheets() {
    let dir = std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod/sourceengine");
    if !dir.is_dir() {
        return;
    }
    let loose = std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod/materials/particle");
    let Ok(entries) = std::fs::read_dir(loose) else { return };
    let mut found = 0;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("vtf"))
            && let Ok(bytes) = std::fs::read(&p)
            && let Some(sheet) = sheet_from_vtf(&bytes)
        {
            found += 1;
            assert!(sheet.sequences.iter().all(|s| !s.frames.is_empty()));
        }
    }
    eprintln!("{found} loose particle textures with a sheet");
}

#[test]
fn pre_aged_particles_start_part_way_through_their_life() {
    let def = system(
        "p",
        vec![burst(3)],
        vec![
            op("Lifetime Pre-Age Noise", vec![("start age minimum", fl(0.5)), ("start age maximum", fl(0.5))]),
            op("Lifetime Random", vec![("lifetime_min", fl(2.0)), ("lifetime_max", fl(2.0))]),
        ],
        vec![op("Lifespan Decay", vec![])],
    );
    let mut sim = Sim::new(Arc::new(Plan::new(&def)), 1);
    sim.step(0.01, [0.0; 3], 10);
    assert!(sim.particles.iter().all(|p| (p.age - 1.01).abs() < 1e-4), "{:?}", sim.particles[0].age);
    assert!(Plan::new(&def).unsupported.is_empty());
}

#[test]
fn random_fades_and_color_fades_ease_in_and_out_by_default() {
    let fade = |ease: Option<bool>| {
        let mut attrs = vec![
            ("fade in time min", fl(1.0)),
            ("fade in time max", fl(1.0)),
            ("proportional 0/1", Value::Bool(false)),
        ];
        if let Some(e) = ease {
            attrs.push(("ease in and out", Value::Bool(e)));
        }
        system(
            "s",
            vec![burst(4)],
            vec![op("Lifetime Random", vec![("lifetime_min", fl(4.0)), ("lifetime_max", fl(4.0))])],
            vec![op("Alpha Fade In Random", attrs), op("Lifespan Decay", vec![])],
        )
    };
    let quarter = |ease| run(&fade(ease), &Points::at([0.0; 3]), 0.25).particles[0].alpha;
    assert!((quarter(Some(false)) - 0.25).abs() < 0.03, "linear");
    assert!((quarter(None) - 0.156).abs() < 0.03, "eased by default: {}", quarter(None));
    assert!((quarter(Some(true)) - quarter(None)).abs() < 1e-4);
    let def = system(
        "c",
        vec![burst(2)],
        vec![
            op("Lifetime Random", vec![("lifetime_min", fl(2.0)), ("lifetime_max", fl(2.0))]),
            op(
                "Color Random",
                vec![("color1", Value::Color([0, 0, 0, 255])), ("color2", Value::Color([0, 0, 0, 255]))],
            ),
        ],
        vec![op("Color Fade", vec![("color_fade", Value::Color([255, 255, 255, 255]))]), op("Lifespan Decay", vec![])],
    );
    let quarter_color = run(&def, &Points::at([0.0; 3]), 0.5).particles[0].color[0];
    assert!((quarter_color - 0.156).abs() < 0.04, "eased: {quarter_color}");
}

#[test]
fn frame_blend_gives_both_frames_and_the_fraction() {
    use crate::sheet::{SheetFrame, SheetSequence};
    let frame = |x: f32| SheetFrame { duration: 1.0, uv: [x, 0.0, x + 0.25, 1.0] };
    let looping = Sheet {
        sequences: vec![SheetSequence {
            number: 0,
            clamp: false,
            frames: vec![frame(0.0), frame(0.25), frame(0.5)],
            total: 3.0,
        }],
    };
    let (a, b, f) = looping.frame_blend(0, 0.5);
    assert_eq!(a, looping.frame_uv(0, 0.5));
    assert_eq!(b[0], 0.5);
    assert!((f - 0.5).abs() < 1e-4, "{f}");
    let (_, wrapped, _) = looping.frame_blend(0, 0.9);
    assert_eq!(wrapped[0], 0.0, "a looping sequence blends the last frame into the first");
    let clamped = Sheet { sequences: vec![SheetSequence { clamp: true, ..looping.sequences[0].clone() }] };
    let (last, same, f) = clamped.frame_blend(0, 1.0);
    assert_eq!(last, same);
    assert_eq!(f, 0.0);
}
