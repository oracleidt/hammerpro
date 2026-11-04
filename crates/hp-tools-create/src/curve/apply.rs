use hp_editor::{CurveRecord, Document, EditError, Id, ObjRef, Owner, SelectOp};
use hp_vmf::Side;

use super::geom::{BuildError, CurveSpec, build_brushes};
use super::meta::CurveMeta;

#[derive(Debug)]
pub enum ApplyError {
    Build(BuildError),
    Edit(EditError),
    Meta,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::Build(e) => e.fmt(f),
            ApplyError::Edit(e) => e.fmt(f),
            ApplyError::Meta => write!(f, "the curve cannot be remembered for editing"),
        }
    }
}

impl From<EditError> for ApplyError {
    fn from(e: EditError) -> Self {
        ApplyError::Edit(e)
    }
}

impl From<BuildError> for ApplyError {
    fn from(e: BuildError) -> Self {
        ApplyError::Build(e)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Target<'a> {
    World,
    NewEntity { class: &'a str, keyvalues: &'a [(String, String)] },
    Entity(Id),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Created {
    pub ids: Vec<Id>,
    pub entity: Option<Id>,
}

pub fn build_meta(
    meta: &CurveMeta,
    material: &str,
    lightmap_scale: i32,
    first_side_id: u32,
) -> Result<Vec<Vec<Side>>, BuildError> {
    let spec = CurveSpec {
        kind: meta.kind,
        points: &meta.points,
        segments: meta.segments,
        thickness: meta.thickness,
        depth: meta.depth,
        material,
        tex_scale: meta.tex_scale,
        lightmap_scale,
        section: meta.section,
    };
    let mut next = first_side_id;
    build_brushes(&spec, &mut || {
        let id = next;
        next = next.saturating_add(1);
        id
    })
}

fn place(doc: &mut Document, solids: Vec<Vec<Side>>, target: Target<'_>) -> Result<Created, EditError> {
    match target {
        Target::World => Ok(Created { ids: doc.create_solids(solids, Owner::World)?, entity: None }),
        Target::NewEntity { class, keyvalues } => {
            let e = doc.create_brush_entity(class, keyvalues, solids)?;
            Ok(Created { ids: doc.scene().entity_solid_ids(e), entity: Some(e) })
        }
        Target::Entity(e) => {
            let ids = doc.create_solids(solids, Owner::Entity(e))?;
            Ok(Created { ids, entity: Some(e) })
        }
    }
}

pub fn create_curve(
    doc: &mut Document,
    meta: &CurveMeta,
    material: &str,
    lightmap_scale: i32,
    target: Target<'_>,
) -> Result<Created, ApplyError> {
    let data = meta.encode().ok_or(ApplyError::Meta)?;
    doc.transaction("Create curve", |doc| {
        let solids = build_meta(meta, material, lightmap_scale, doc.peek_next_side_id())?;
        let created = place(doc, solids, target)?;
        doc.tag_curve_pieces(&created.ids, &data)?;
        Ok::<_, ApplyError>(created)
    })
}

pub fn replace_curve(
    doc: &mut Document,
    old: &CurveRecord,
    meta: &CurveMeta,
    material: &str,
    lightmap_scale: i32,
) -> Result<Created, ApplyError> {
    let data = meta.encode().ok_or(ApplyError::Meta)?;
    doc.transaction("Edit curve", |doc| {
        let solids = build_meta(meta, material, lightmap_scale, doc.peek_next_side_id())?;
        let target = match old.owner {
            Owner::World => Target::World,
            Owner::Entity(e) => Target::Entity(e),
        };
        let created = place(doc, solids, target)?;
        doc.tag_curve_pieces(&created.ids, &data)?;
        let gone: Vec<ObjRef> = old.ids.iter().map(|id| ObjRef::Solid(*id)).collect();
        doc.delete(&gone)?;
        let selected: Vec<ObjRef> = match created.entity {
            Some(e) => vec![ObjRef::Entity(e)],
            None => created.ids.iter().map(|id| ObjRef::Solid(*id)).collect(),
        };
        doc.select(&selected, SelectOp::Replace);
        Ok::<_, ApplyError>(created)
    })
}

pub fn material_of(doc: &Document, record: &CurveRecord) -> Option<String> {
    record
        .ids
        .iter()
        .filter_map(|id| doc.scene().solid(*id))
        .find_map(|n| n.solid.sides.first().map(|s| s.material.clone()))
}

pub fn read_curve(doc: &Document, piece: Id) -> Option<(CurveRecord, CurveMeta)> {
    let record = doc.curve_record(piece)?;
    let meta = CurveMeta::decode(&record.data)?;
    Some((record, meta))
}
