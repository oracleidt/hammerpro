use glam::DVec3;

use crate::Model;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Collision {
    #[default]
    Pieces,
    Box,
    None,
}

#[derive(Clone, Debug, PartialEq)]
pub struct QcOptions {
    pub model: String,
    pub cdmaterials: String,
    pub static_prop: bool,
    pub surface_prop: String,
    pub mass: f64,
    pub collision: Collision,
}

pub fn material_file_name(name: &str, index: usize) -> String {
    let mut s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_owned();
    if s.is_empty() {
        s = format!("material_{index}");
    }
    s.truncate(48);
    s
}

fn header() -> String {
    "version 1\nnodes\n0 \"root\" -1\nend\nskeleton\ntime 0\n0 0 0 0 0 0 0\nend\ntriangles\n".to_owned()
}

pub fn to_smd_space(v: DVec3) -> DVec3 {
    DVec3::new(v.y, -v.x, v.z)
}

fn vertex(s: &mut String, p: DVec3, n: DVec3, u: f64, v: f64) {
    use std::fmt::Write;
    let (p, n) = (to_smd_space(p), to_smd_space(n));
    let _ = writeln!(s, "0 {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6} {:.6}", p.x, p.y, p.z, n.x, n.y, n.z, u, 1.0 - v);
}

pub fn reference_smd(model: &Model, names: &[String]) -> String {
    let mut s = header();
    for t in &model.triangles {
        s.push_str(names.get(t.material).map_or("default", String::as_str));
        s.push('\n');
        for k in 0..3 {
            vertex(&mut s, t.pos[k], t.normal[k], t.uv[k].x, t.uv[k].y);
        }
    }
    s.push_str("end\n");
    s
}

pub fn collision_smd(model: &Model, kind: Collision) -> Option<String> {
    match kind {
        Collision::None => None,
        Collision::Pieces => {
            let mut s = header();
            for t in &model.triangles {
                s.push_str("phys\n");
                for k in 0..3 {
                    vertex(&mut s, t.pos[k], t.normal[k], 0.0, 0.0);
                }
            }
            s.push_str("end\n");
            Some(s)
        }
        Collision::Box => {
            let (lo, hi) = model.bounds()?;
            let c = |i: usize| {
                DVec3::new(
                    if i & 1 == 0 { lo.x } else { hi.x },
                    if i & 2 == 0 { lo.y } else { hi.y },
                    if i & 4 == 0 { lo.z } else { hi.z },
                )
            };
            let quads: [([usize; 4], DVec3); 6] = [
                ([0, 2, 6, 4], -DVec3::X),
                ([1, 5, 7, 3], DVec3::X),
                ([0, 4, 5, 1], -DVec3::Y),
                ([2, 3, 7, 6], DVec3::Y),
                ([0, 1, 3, 2], -DVec3::Z),
                ([4, 6, 7, 5], DVec3::Z),
            ];
            let mut s = header();
            for (q, n) in quads {
                for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
                    s.push_str("phys\n");
                    for i in tri {
                        vertex(&mut s, c(i), n, 0.0, 0.0);
                    }
                }
            }
            s.push_str("end\n");
            Some(s)
        }
    }
}

pub fn qc(opts: &QcOptions, stem: &str, has_collision: bool) -> String {
    let mut s = String::new();
    s.push_str(&format!("$modelname \"{}.mdl\"\n", opts.model.trim_matches('/')));
    if opts.static_prop {
        s.push_str("$staticprop\n");
    }
    let surface = if opts.surface_prop.trim().is_empty() { "default" } else { opts.surface_prop.trim() };
    s.push_str(&format!("$surfaceprop \"{surface}\"\n"));
    s.push_str(&format!("$cdmaterials \"{}/\"\n", opts.cdmaterials.trim_matches('/')));
    s.push_str(&format!("$body \"body\" \"{stem}_ref.smd\"\n"));
    s.push_str(&format!("$sequence \"idle\" \"{stem}_ref.smd\" fps 30\n"));
    if has_collision {
        s.push_str(&format!("$collisionmodel \"{stem}_phys.smd\"\n{{\n"));
        if opts.collision == Collision::Pieces {
            s.push_str("\t$concave\n\t$maxconvexpieces 40\n");
        }
        let mass = if opts.mass.is_finite() && opts.mass > 0.0 { opts.mass } else { 10.0 };
        s.push_str(&format!("\t$mass {mass:.2}\n}}\n"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Material, Triangle, UpAxis};
    use glam::DVec2;

    fn model() -> Model {
        Model {
            triangles: vec![Triangle {
                pos: [DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0), DVec3::new(0.0, 10.0, 5.0)],
                normal: [DVec3::Z; 3],
                uv: [DVec2::new(0.0, 0.0), DVec2::new(1.0, 0.0), DVec2::new(0.0, 1.0)],
                material: 0,
            }],
            materials: vec![Material { name: "Wood Dark!".to_owned(), ..Material::default() }],
            up: UpAxis::Z,
            meters_per_unit: 1.0,
            objects: Vec::new(),
        }
    }

    #[test]
    fn smd_and_qc() {
        let m = model();
        let names: Vec<String> = m.materials.iter().enumerate().map(|(i, x)| material_file_name(&x.name, i)).collect();
        assert_eq!(names, ["wood_dark"]);
        let smd = reference_smd(&m, &names);
        assert!(smd.starts_with("version 1\n") && smd.ends_with("end\n"));
        assert_eq!(smd.matches("wood_dark\n").count(), 1);
        let line = smd.lines().find(|l| l.starts_with("0 0.000000 -10.000000")).expect("vertex");
        assert_eq!(line.split_whitespace().count(), 9);
        assert!(line.ends_with(" 1.000000 1.000000"), "V flipped: {line}");
        let boxed = collision_smd(&m, Collision::Box).expect("box");
        assert_eq!(boxed.matches("phys\n").count(), 12);
        assert!(collision_smd(&m, Collision::None).is_none());
        let opts = QcOptions {
            model: "hammerpro/chair".to_owned(),
            cdmaterials: "models/hammerpro/chair".to_owned(),
            static_prop: true,
            surface_prop: "wood".to_owned(),
            mass: 20.0,
            collision: Collision::Pieces,
        };
        let q = qc(&opts, "chair", true);
        assert!(q.contains("$modelname \"hammerpro/chair.mdl\""));
        assert!(q.contains("$staticprop"));
        assert!(q.contains("$cdmaterials \"models/hammerpro/chair/\""));
        assert!(q.contains("$collisionmodel \"chair_phys.smd\"") && q.contains("$concave"));
        let physics = qc(&QcOptions { static_prop: false, ..opts }, "chair", false);
        assert!(!physics.contains("$staticprop") && !physics.contains("$collisionmodel"));
    }
}
