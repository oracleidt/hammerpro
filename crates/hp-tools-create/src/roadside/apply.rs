use glam::{DVec2, DVec3};
use hp_app::ViewKind;
use hp_editor::{Document, EditError, Id, ObjRef, RoadsideRecord, SelectOp};
use hp_geom::roadside::{Feature, Profile, RoadsideError, Spine, build};
use hp_geom::{GeomError, hull_solid};
use hp_vmf::Side;

use super::meta::{Kind, RoadsideMeta};
use super::source::resolve_frame;
use crate::curve::apply::{Created, Target};

#[derive(Debug)]
pub enum ApplyError {
    Build(RoadsideError),
    Geom(GeomError),
    Edit(EditError),
    Meta,
}

impl std::fmt::Display for ApplyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApplyError::Build(e) => e.fmt(f),
            ApplyError::Geom(e) => e.fmt(f),
            ApplyError::Edit(e) => e.fmt(f),
            ApplyError::Meta => write!(f, "the set cannot be remembered for editing"),
        }
    }
}

impl From<EditError> for ApplyError {
    fn from(e: EditError) -> Self {
        ApplyError::Edit(e)
    }
}

impl From<RoadsideError> for ApplyError {
    fn from(e: RoadsideError) -> Self {
        ApplyError::Build(e)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Sidewalk,
    Curb,
    Gutter,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub part: Part,
    pub points: Vec<DVec3>,
}

pub fn features(meta: &RoadsideMeta) -> Result<Vec<(Part, Feature)>, RoadsideError> {
    let radius_at = |offset: f64| (meta.radius - offset).max(0.0);
    let feature = |profile: Profile, gap: f64| Feature { sides: meta.sides, gap, radius: radius_at(gap), profile };
    match meta.kind {
        Kind::Sidewalk => Ok(vec![(Part::Sidewalk, feature(Profile::Sidewalk(meta.sidewalk), meta.gap))]),
        Kind::Curb => {
            if !meta.use_curb && !meta.use_gutter {
                return Err(RoadsideError::BadValue("turn on the curb, the gutter or both"));
            }
            let mut offset = meta.gap;
            let mut out = Vec::new();
            if meta.use_gutter {
                out.push((Part::Gutter, feature(Profile::Gutter(meta.gutter), offset)));
                offset += meta.gutter.width;
            }
            if meta.use_curb {
                out.push((Part::Curb, feature(Profile::Curb(meta.curb), offset)));
            }
            Ok(out)
        }
    }
}

pub fn build_pieces(meta: &RoadsideMeta, spines: &[Spine]) -> Result<Vec<Piece>, RoadsideError> {
    if !(meta.radius.is_finite() && meta.gap.is_finite()) {
        return Err(RoadsideError::BadValue("the gap and the corner radius must be numbers"));
    }
    let mut out = Vec::new();
    for (part, feature) in features(meta)? {
        for points in build(spines, &feature)? {
            out.push(Piece { part, points });
        }
    }
    Ok(out)
}

fn material_of(meta: &RoadsideMeta, part: Part) -> String {
    let m = &meta.materials;
    match part {
        Part::Sidewalk => m.top.clone(),
        Part::Curb => m.or_top(&m.curb),
        Part::Gutter => m.or_top(&m.gutter),
    }
}

pub fn to_world(kind: ViewKind, p: DVec3) -> DVec3 {
    kind.from_2d(DVec2::new(p.x, p.y), p.z)
}

pub fn to_local(kind: ViewKind, p: DVec3) -> DVec3 {
    match kind {
        ViewKind::Front => DVec3::new(p.y, p.z, p.x),
        ViewKind::Side => DVec3::new(p.x, p.z, p.y),
        _ => p,
    }
}

fn local_side(kind: ViewKind, side: &Side) -> Side {
    let mut s = side.clone();
    s.plane = side.plane.map(|p| to_local(kind, p));
    s
}

fn outward_normal(sides: &[Side], side: &Side) -> Option<DVec3> {
    let n = (side.plane[1] - side.plane[0]).cross(side.plane[2] - side.plane[0]).try_normalize()?;
    let centroid = sides.iter().flat_map(|s| s.plane).sum::<DVec3>() / (sides.len() * 3) as f64;
    Some(if n.dot(centroid - side.plane[0]) > 0.0 { -n } else { n })
}

fn face_toward_road(sides: &[Side], side: &Side, spines: &[Spine]) -> bool {
    let Some(n) = outward_normal(sides, side) else { return false };
    if n.z.abs() > 0.5 {
        return false;
    }
    let p = (side.plane[0] + side.plane[1] + side.plane[2]) / 3.0;
    let xy = DVec2::new(p.x, p.y);
    let Some(nearest) = spines.iter().map(|s| s.nearest(xy)).min_by(|a, b| a.dist.total_cmp(&b.dist)) else {
        return false;
    };
    let to_road = nearest.point - xy;
    let flat = DVec2::new(n.x, n.y);
    let (la, lb) = (flat.length(), to_road.length());
    la > 1e-9 && lb > 1e-9 && flat.dot(to_road) > 0.5 * la * lb
}

pub fn build_solids(
    meta: &RoadsideMeta,
    spines: &[Spine],
    kind: ViewKind,
    lightmap_scale: i32,
    first_side_id: u32,
) -> Result<Vec<Vec<Side>>, ApplyError> {
    let pieces = build_pieces(meta, spines)?;
    let mut next = first_side_id;
    let mut out = Vec::with_capacity(pieces.len());
    for piece in &pieces {
        let material = material_of(meta, piece.part);
        let world: Vec<DVec3> = piece.points.iter().map(|p| to_world(kind, *p)).collect();
        let mut sides = hull_solid(&world, &material, meta.tex_scale, lightmap_scale, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        })
        .map_err(ApplyError::Geom)?;
        let face = &meta.materials.face;
        if !face.is_empty() && piece.part != Part::Gutter {
            let snapshot: Vec<Side> = sides.iter().map(|s| local_side(kind, s)).collect();
            for (s, local) in sides.iter_mut().zip(&snapshot) {
                if face_toward_road(&snapshot, local, spines) {
                    s.material.clone_from(face);
                }
            }
        }
        out.push(sides);
    }
    Ok(out)
}

fn place(doc: &mut Document, solids: Vec<Vec<Side>>, target: Target<'_>) -> Result<Created, EditError> {
    match target {
        Target::World => Ok(Created { ids: doc.create_solids(solids, hp_editor::Owner::World)?, entity: None }),
        Target::NewEntity { class, keyvalues } => {
            let e = doc.create_brush_entity(class, keyvalues, solids)?;
            Ok(Created { ids: doc.scene().entity_solid_ids(e), entity: Some(e) })
        }
        Target::Entity(e) => {
            let ids = doc.create_solids(solids, hp_editor::Owner::Entity(e))?;
            Ok(Created { ids, entity: Some(e) })
        }
    }
}

fn select_created(doc: &mut Document, created: &Created) {
    let selected: Vec<ObjRef> = match created.entity {
        Some(e) => vec![ObjRef::Entity(e)],
        None => created.ids.iter().map(|id| ObjRef::Solid(*id)).collect(),
    };
    doc.select(&selected, SelectOp::Replace);
}

pub fn undo_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Sidewalk => "Create sidewalk",
        Kind::Curb => "Create curb",
    }
}

pub fn create(
    doc: &mut Document,
    meta: &RoadsideMeta,
    lightmap_scale: i32,
    target: Target<'_>,
) -> Result<Created, ApplyError> {
    let (spines, kind) = resolve_frame(doc, &meta.sources)?;
    let data = meta.encode();
    doc.transaction(undo_label(meta.kind), |doc| {
        let solids = build_solids(meta, &spines, kind, lightmap_scale, doc.peek_next_side_id())?;
        let created = place(doc, solids, target)?;
        doc.tag_roadside_pieces(&created.ids, &data)?;
        select_created(doc, &created);
        Ok::<_, ApplyError>(created)
    })
}

pub fn replace(
    doc: &mut Document,
    old: &RoadsideRecord,
    meta: &RoadsideMeta,
    lightmap_scale: i32,
) -> Result<Created, ApplyError> {
    let (spines, kind) = resolve_frame(doc, &meta.sources)?;
    let data = meta.encode();
    doc.transaction("Edit roadside", |doc| {
        let solids = build_solids(meta, &spines, kind, lightmap_scale, doc.peek_next_side_id())?;
        let target = match old.owner {
            hp_editor::Owner::World => Target::World,
            hp_editor::Owner::Entity(e) => Target::Entity(e),
        };
        let created = place(doc, solids, target)?;
        doc.tag_roadside_pieces(&created.ids, &data)?;
        let gone: Vec<ObjRef> = old.ids.iter().map(|id| ObjRef::Solid(*id)).collect();
        doc.delete(&gone)?;
        select_created(doc, &created);
        Ok::<_, ApplyError>(created)
    })
}

pub fn material_names(doc: &Document, record: &RoadsideRecord) -> Option<String> {
    record
        .ids
        .iter()
        .filter_map(|id| doc.scene().solid(*id))
        .find_map(|n| n.solid.sides.first().map(|s| s.material.clone()))
}

pub fn read(doc: &Document, piece: Id) -> Option<(RoadsideRecord, RoadsideMeta)> {
    let record = doc.roadside_record(piece)?;
    let meta = RoadsideMeta::decode(&record.data)?;
    Some((record, meta))
}

pub fn piece_of(doc: &Document, id: Id) -> Option<Id> {
    if doc.roadside_tag(id).is_some() {
        return Some(id);
    }
    doc.scene().entity_solid_ids(id).into_iter().find(|s| doc.roadside_tag(*s).is_some())
}

pub fn lightmap_of(doc: &Document, record: &RoadsideRecord) -> i32 {
    record
        .ids
        .iter()
        .filter_map(|id| doc.scene().solid(*id))
        .find_map(|n| n.solid.sides.first().map(|s| s.lightmap_scale))
        .filter(|l| *l > 0)
        .unwrap_or(16)
}
