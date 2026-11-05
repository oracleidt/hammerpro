use hp_editor::{Document, EditError, Id, ObjRef, Owner, SelectOp};
use hp_geom::hull_solid;

use super::model::{PieceKind, PipePlan};

#[derive(Clone, Debug, PartialEq)]
pub struct PipeCreated {
    pub ids: Vec<Id>,
    pub group: Option<Id>,
}

pub fn create(doc: &mut Document, plan: &PipePlan, lightmap: i32, group: bool) -> Result<PipeCreated, EditError> {
    doc.transaction("Create pipe", |doc| create_in(doc, plan, lightmap, group))
}

pub fn create_in(doc: &mut Document, plan: &PipePlan, lightmap: i32, group: bool) -> Result<PipeCreated, EditError> {
    let spec = &plan.spec;
    let collar_material = spec.collar_material.as_deref().unwrap_or(&spec.material);
    let mut next = doc.peek_next_side_id();
    let mut solids = Vec::with_capacity(plan.pieces.len());
    for piece in &plan.pieces {
        let material = if piece.kind == PieceKind::Collar { collar_material } else { spec.material.as_str() };
        let sides = hull_solid(&piece.points, material, spec.tex_scale, lightmap, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        })?;
        solids.push(sides);
    }
    let ids = doc.create_solids(solids, Owner::World)?;
    let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Solid(*id)).collect();
    let group = if group && objs.len() > 1 { Some(doc.group(&objs)?) } else { None };
    doc.select(&objs, SelectOp::Replace);
    Ok(PipeCreated { ids, group })
}
