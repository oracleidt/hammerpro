use glam::DVec3;
use hp_geom::{SolidGeom, build_solid};
use hp_vmf::{Entity, Map, Side, Solid};

pub const CLASS: &str = "func_breakable_surf";

pub const NODRAW: &str = "tools/toolsnodraw";

pub const PANEL_SIZE: f64 = 12.0;

pub const STOCK_GLASS: &str = "glass/glasswindowbreak070b";

pub const CORNER_KEYS: [&str; 4] = ["upperleft", "upperright", "lowerleft", "lowerright"];

const SIZE_EPS: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuadError {
    None = 0,
    MultipleFaces = 1,
    NotQuad = 2,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Quad {
    pub upper_left: DVec3,
    pub upper_right: DVec3,
    pub lower_left: DVec3,
    pub lower_right: DVec3,
    pub error: QuadError,
    pub textured_faces: usize,
    pub solid: hp_vmf::Id,
    pub side_index: usize,
    pub material: String,
    pub vertex_count: usize,
}

impl Quad {
    pub fn width(&self) -> f64 {
        self.lower_right.distance(self.lower_left)
    }

    pub fn height(&self) -> f64 {
        self.upper_left.distance(self.lower_left)
    }

    pub fn corners(&self) -> [DVec3; 4] {
        [self.lower_left, self.upper_left, self.upper_right, self.lower_right]
    }

    pub fn too_small(&self) -> bool {
        self.width() + SIZE_EPS < PANEL_SIZE || self.height() + SIZE_EPS < PANEL_SIZE
    }
}

pub fn is_breakable_surf(classname: &str) -> bool {
    classname.trim().eq_ignore_ascii_case(CLASS)
}

pub fn is_nodraw(material: &str) -> bool {
    crate::glob::same_material(material, NODRAW)
}

pub fn compute<'a>(solids: impl IntoIterator<Item = (&'a Solid, &'a SolidGeom)>) -> Option<Quad> {
    let mut best: Option<(f64, &Solid, usize, Vec<DVec3>)> = None;
    let mut textured = 0usize;
    for (solid, geom) in solids {
        for face in &geom.faces {
            let Some(side) = solid.sides.get(face.side_index) else { continue };
            if is_nodraw(&side.material) || face.vertices.len() < 3 {
                continue;
            }
            textured += 1;
            let area = face.area();
            if best.as_ref().is_none_or(|(a, ..)| area > *a + 1e-9) {
                best = Some((area, solid, face.side_index, face.vertices.clone()));
            }
        }
    }
    let (_, solid, side_index, verts) = best?;
    let side = &solid.sides[side_index];
    let (ll, ul, ur, lr) = corners(side, &verts);
    let error = if textured > 1 {
        QuadError::MultipleFaces
    } else if verts.len() != 4 {
        QuadError::NotQuad
    } else {
        QuadError::None
    };
    Some(Quad {
        upper_left: ul,
        upper_right: ur,
        lower_left: ll,
        lower_right: lr,
        error,
        textured_faces: textured,
        solid: solid.id,
        side_index,
        material: side.material.clone(),
        vertex_count: verts.len(),
    })
}

fn tex_axis(axis: DVec3, scale: f64, normal: DVec3) -> Option<DVec3> {
    let flat = axis - normal * axis.dot(normal);
    let len = flat.length();
    (len > 0.1 && len.is_finite()).then(|| flat / len * if scale < 0.0 { -1.0 } else { 1.0 })
}

fn corners(side: &Side, verts: &[DVec3]) -> (DVec3, DVec3, DVec3, DVec3) {
    let normal = hp_geom::Plane::from_side(side).map_or(DVec3::Z, |p| p.normal);
    let u = tex_axis(side.uaxis.axis, side.uaxis.scale, normal);
    let v = tex_axis(side.vaxis.axis, side.vaxis.scale, normal);
    let (u, v) = match (u, v) {
        (Some(u), Some(v)) if u.cross(v).length() > 0.1 => (u, v),
        (Some(u), _) => (u, normal.cross(u)),
        _ => {
            let edge = verts.get(1).copied().unwrap_or(DVec3::X) - verts.first().copied().unwrap_or(DVec3::ZERO);
            let e = edge.try_normalize().unwrap_or(DVec3::X);
            (e, normal.cross(e))
        }
    };
    let uv: Vec<(f64, f64)> = verts.iter().map(|p| (p.dot(u), p.dot(v))).collect();
    let pick = |score: &dyn Fn(f64, f64) -> f64, skip: &[usize]| -> usize {
        let mut best = usize::MAX;
        let mut best_score = f64::NEG_INFINITY;
        for (i, (a, b)) in uv.iter().enumerate() {
            if skip.contains(&i) {
                continue;
            }
            let s = score(*a, *b);
            if s > best_score + 1e-9 {
                best_score = s;
                best = i;
            }
        }
        best
    };
    let ul = pick(&|a, b| -(a + b), &[]);
    let lr = pick(&|a, b| a + b, &[ul]);
    let ur = pick(&|a, b| a - b, &[ul, lr]);
    let ll = pick(&|a, b| b - a, &[ul, lr, ur]);
    let at = |i: usize| verts.get(i).copied().unwrap_or(verts[0]);
    (at(ll), at(ul), at(ur), at(lr))
}

pub fn format_vec(v: DVec3) -> String {
    let f = |x: f64| {
        let r = (x * 1000.0).round() / 1000.0;
        let r = if r == 0.0 { 0.0 } else { r };
        if r.fract() == 0.0 && r.abs() < 1e15 { format!("{}", r as i64) } else { format!("{r}") }
    };
    format!("{} {} {}", f(v.x), f(v.y), f(v.z))
}

fn parse_vec(s: &str) -> Option<DVec3> {
    let mut it = s.split_ascii_whitespace().map(|t| t.parse::<f64>().ok());
    let v = DVec3::new(it.next()??, it.next()??, it.next()??);
    v.is_finite().then_some(v)
}

pub fn keyvalues(quad: &Quad) -> [(&'static str, String); 5] {
    [
        ("upperleft", format_vec(quad.upper_left)),
        ("upperright", format_vec(quad.upper_right)),
        ("lowerleft", format_vec(quad.lower_left)),
        ("lowerright", format_vec(quad.lower_right)),
        ("error", (quad.error as i32).to_string()),
    ]
}

pub fn apply_quad(entity: &mut Entity, quad: &Quad) -> bool {
    let mut changed = false;
    for (key, value) in keyvalues(quad) {
        let same = match entity.get(key) {
            Some(old) if key == "error" => old.trim().parse::<i32>().ok() == value.parse::<i32>().ok(),
            Some(old) => parse_vec(old).zip(parse_vec(&value)).is_some_and(|(a, b)| a.distance(b) < 1e-3),
            None => false,
        };
        if !same {
            entity.set(key, value);
            changed = true;
        }
    }
    changed
}

pub fn apply_own_solids(entity: &mut Entity) -> bool {
    if !is_breakable_surf(&entity.classname) || entity.solids.is_empty() {
        return false;
    }
    let geoms: Vec<SolidGeom> = entity.solids.iter().map(|s| build_solid(&s.sides)).collect();
    let quad = compute(entity.solids.iter().zip(geoms.iter()));
    match quad {
        Some(q) => apply_quad(entity, &q),
        None => false,
    }
}

pub fn prepare_map(map: &mut Map) -> usize {
    map.entities.iter_mut().filter(|e| is_breakable_surf(&e.classname)).map(apply_own_solids).filter(|c| *c).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn box_sides(lo: DVec3, hi: DVec3, glass_face: Option<usize>, glass: &str) -> Vec<Side> {
        let mut sides = crate::tests::box_sides(lo, hi, glass);
        for (i, s) in sides.iter_mut().enumerate() {
            s.id = i as u32 + 1;
            if glass_face.is_some_and(|g| g != i) {
                s.material = NODRAW.to_uppercase();
            }
        }
        sides
    }

    fn window(glass_face: Option<usize>) -> Entity {
        let solid = Solid {
            id: 10,
            sides: box_sides(DVec3::new(0.0, 0.0, 0.0), DVec3::new(96.0, 2.0, 72.0), glass_face, STOCK_GLASS),
            ..Solid::default()
        };
        Entity { id: 1, classname: CLASS.to_owned(), solids: vec![solid], ..Entity::default() }
    }

    #[test]
    fn corners_follow_the_glass_face_like_hammer() {
        let mut e = window(Some(3));
        assert!(apply_own_solids(&mut e));
        assert_eq!(e.get("lowerleft"), Some("0 0 0"));
        assert_eq!(e.get("upperleft"), Some("0 0 72"));
        assert_eq!(e.get("upperright"), Some("96 0 72"));
        assert_eq!(e.get("lowerright"), Some("96 0 0"));
        assert_eq!(e.get("error"), Some("0"));
        assert!(!apply_own_solids(&mut e), "a second pass keeps the keys");
        let geom = build_solid(&e.solids[0].sides);
        let q = compute([(&e.solids[0], &geom)]).expect("quad");
        assert_eq!((q.width(), q.height()), (96.0, 72.0));
        assert!(!q.too_small());
        assert_eq!(q.side_index, 3);
    }

    #[test]
    fn stale_or_missing_corners_are_rewritten() {
        let mut e = window(Some(2));
        e.set("upperleft", "0 0 0");
        e.set("lowerright", "0 0 0");
        e.set("error", "2");
        let mut map = Map { entities: vec![e], ..Map::default() };
        assert_eq!(prepare_map(&mut map), 1);
        let e = &map.entities[0];
        assert_eq!(e.get("upperleft"), Some("0 2 72"));
        assert_eq!(e.get("lowerright"), Some("96 2 0"));
        assert_eq!(e.get("error"), Some("0"));
    }

    #[test]
    fn errors_match_the_hammer_codes() {
        let mut all = window(None);
        apply_own_solids(&mut all);
        assert_eq!(all.get("error"), Some("1"));
        let mut thin = window(Some(3));
        thin.solids[0].sides = box_sides(DVec3::ZERO, DVec3::new(8.0, 2.0, 72.0), Some(3), STOCK_GLASS);
        let geom = build_solid(&thin.solids[0].sides);
        assert!(compute([(&thin.solids[0], &geom)]).expect("quad").too_small());
        let mut other = Entity { classname: "func_brush".into(), ..window(Some(3)) };
        assert!(!apply_own_solids(&mut other));
        assert!(other.get("upperleft").is_none());
    }

    struct Crackless;

    impl crate::limits::checks::CheckKnowledge for Crackless {
        fn material_has_crack(&self, name: &str) -> Option<bool> {
            Some(crate::glob::same_material(name, STOCK_GLASS))
        }
    }

    fn problems(doc: &crate::Document) -> Vec<crate::Problem> {
        let snap = crate::limits::snapshot::MapSnapshot::capture(doc);
        crate::limits::checks::check_snapshot(&snap, &Crackless, &crate::limits::checks::CheckOptions::default())
    }

    #[test]
    fn document_saves_corners_and_fixes_the_setup() {
        let mut map = Map::new_empty();
        map.entities.push(window(None));
        let mut doc = crate::Document::from_map(map);
        let text = doc.to_vmf_string();
        assert!(text.contains("\"error\" \"1\""), "{text}");
        let found = problems(&doc);
        let faces = found.iter().find(|p| p.code == crate::limits::breakable::CODE_FACES).expect("faces problem");
        assert!(faces.fixable);
        assert!(found.iter().all(|p| p.code != crate::limits::breakable::CODE_CRACK));
        assert!(doc.fix_problem(&faces.clone()).expect("fix"));
        let text = doc.to_vmf_string();
        assert!(text.contains("\"error\" \"0\""), "{text}");
        assert!(text.contains("\"upperleft\" \""), "{text}");
        let found = problems(&doc);
        assert!(found.iter().all(|p| !p.code.starts_with("breakable_surf")), "{found:?}");

        let ids = doc.scene().entity_solid_ids(1);
        let mut s = doc.scene().solid(ids[0]).expect("solid").solid.clone();
        for side in &mut s.sides {
            if !is_nodraw(&side.material) {
                side.material = "glass/plain".into();
            }
        }
        doc.update_solids(vec![(s.id, s)]);
        let crack = problems(&doc).into_iter().find(|p| p.code == crate::limits::breakable::CODE_CRACK).expect("crack");
        assert!(doc.fix_problem(&crack).expect("fix crack"));
        assert!(problems(&doc).iter().all(|p| !p.code.starts_with("breakable_surf")));
    }

    #[test]
    fn number_format() {
        assert_eq!(format_vec(DVec3::new(-0.0, 1.5, -64.0004)), "0 1.5 -64");
    }
}
