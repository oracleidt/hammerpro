use std::collections::HashMap;

use super::checks::CheckKnowledge;
use super::snapshot::{MapSnapshot, SolidRec};
use crate::Id;
use crate::problems::{Problem, Severity};
use crate::quadbounds::{PANEL_SIZE, is_breakable_surf, is_nodraw};
use crate::scene::Owner;
use crate::selection::{FaceRef, ObjRef};

pub const CODE_FACES: &str = "breakable_surf_faces";
pub const CODE_NOT_QUAD: &str = "breakable_surf_not_quad";
pub const CODE_SIZE: &str = "breakable_surf_size";
pub const CODE_CRACK: &str = "breakable_surf_crack";

fn fmt(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r}") }
}

fn entity_key<'a>(snap: &'a MapSnapshot, id: Id, key: &str) -> Option<&'a str> {
    snap.entities.iter().find(|r| r.entity.id == id).and_then(|r| r.entity.get(key))
}

pub(crate) fn check_breakable_surfs(snap: &MapSnapshot, k: &dyn CheckKnowledge, out: &mut Vec<Problem>) {
    let ids: Vec<(Id, &str)> = snap
        .entities
        .iter()
        .filter(|r| is_breakable_surf(&r.entity.classname))
        .map(|r| (r.entity.id, r.entity.classname.as_str()))
        .collect();
    if ids.is_empty() {
        return;
    }
    let mut solids: HashMap<Id, Vec<&SolidRec>> = HashMap::new();
    for s in &snap.solids {
        if let Owner::Entity(e) = s.owner
            && ids.iter().any(|(id, _)| *id == e)
        {
            solids.entry(e).or_default().push(s);
        }
    }
    for (id, class) in ids {
        let Some(list) = solids.get(&id) else { continue };
        let mut faces: Vec<(FaceRef, f64, usize, f64, f64, u32)> = Vec::new();
        for s in list {
            for f in &s.geom.faces {
                let Some(side) = s.sides.get(f.side_index) else { continue };
                if is_nodraw(snap.material_key(side.material)) || f.vertices.len() < 3 {
                    continue;
                }
                let v = &f.vertices;
                let a = v[1].distance(v[0]);
                let b = v[2].distance(v[1]);
                faces.push((FaceRef::new(s.id, f.side_index), f.area(), v.len(), a, b, side.material));
            }
        }
        let obj = vec![ObjRef::Entity(id)];
        let eid = id.to_string();
        if faces.len() != 1 {
            let mut p = Problem::new(
                Severity::Error,
                CODE_FACES,
                format!(
                    "Breakable glass {id} ({class}) has {} faces that are not nodraw: it needs exactly one, or the game removes it or breaks it in one piece",
                    faces.len()
                ),
                obj.clone(),
                faces.len() > 1,
            )
            .with_args(vec![eid.clone(), class.to_owned(), faces.len().to_string()]);
            p.faces = faces.iter().map(|f| f.0).collect();
            out.push(p);
        }
        let Some(best) = faces.iter().max_by(|a, b| a.1.total_cmp(&b.1)) else { continue };
        let (face, _, count, a, b, material) = *best;
        if count != 4 {
            let mut p = Problem::new(
                Severity::Error,
                CODE_NOT_QUAD,
                format!(
                    "Breakable glass {id} ({class}) has a glass face with {count} corners: it must be a four sided quad"
                ),
                obj.clone(),
                false,
            )
            .with_args(vec![eid.clone(), class.to_owned(), count.to_string()]);
            p.faces = vec![face];
            out.push(p);
        } else if a.min(b) + 0.01 < PANEL_SIZE {
            out.push(
                Problem::new(
                    Severity::Warning,
                    CODE_SIZE,
                    format!(
                        "Breakable glass {id} ({class}) is {} x {} units: a side under {} units holds no glass panel, so the whole pane breaks in one piece",
                        fmt(a),
                        fmt(b),
                        fmt(PANEL_SIZE)
                    ),
                    obj.clone(),
                    false,
                )
                .with_args(vec![eid.clone(), fmt(a), fmt(b), fmt(PANEL_SIZE)]),
            );
        }
        let name = snap.material_display(material);
        let tile = entity_key(snap, id, "surfacetype").is_some_and(|v| v.trim() == "1");
        if !tile && k.material_has_crack(name) == Some(false) {
            let mut p = Problem::new(
                Severity::Warning,
                CODE_CRACK,
                format!(
                    "Breakable glass {id} ({class}) uses {name}, which is not a breakable glass material (no $crackmaterial, no ShatteredGlass shader): the game cannot draw the broken pane"
                ),
                obj,
                true,
            )
            .with_args(vec![eid, class.to_owned(), name.to_owned()]);
            p.faces = vec![face];
            out.push(p);
        }
    }
}
