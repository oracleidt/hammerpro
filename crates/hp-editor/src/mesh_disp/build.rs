use glam::DVec3;
use hp_geom::{Aabb, DispMesh, Plane, Primitive, PrimitiveParams, build_disp, create_disp_with_axes, make_primitive};
use hp_vmf::{DispInfo, Side, Solid};

use super::{Frame, MeshDispOptions, TilePlan, check_options};
use crate::document::Document;
use crate::selection::{FaceRef, ObjRef};
use crate::unprop::NODRAW;
use crate::{EditError, EditResult, Id, Owner};

pub(super) const UNDO_MESH_TO_DISP: &str = "Mesh to displacement";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshDispResult {
    pub solids: Vec<Id>,
    pub faces: Vec<FaceRef>,
    pub tiles: usize,
    pub skirts: usize,
    pub bottoms: usize,
    pub group: Option<Id>,
}

pub(super) type Shape<'a> = Box<dyn Fn(&mut DispInfo, &DispMesh) + 'a>;

pub(super) struct Piece<'a> {
    pub sides: Vec<Side>,
    pub disp_side: Option<usize>,
    pub power: u32,
    pub shape: Option<Shape<'a>>,
}

pub(super) fn local_box(frame: &Frame, u: (f64, f64), v: (f64, f64), h: (f64, f64)) -> Aabb {
    let corners = [
        DVec3::new(u.0, v.0, h.0),
        DVec3::new(u.1, v.0, h.0),
        DVec3::new(u.0, v.1, h.0),
        DVec3::new(u.1, v.1, h.0),
        DVec3::new(u.0, v.0, h.1),
        DVec3::new(u.1, v.0, h.1),
        DVec3::new(u.0, v.1, h.1),
        DVec3::new(u.1, v.1, h.1),
    ];
    Aabb::from_points(corners.map(|c| frame.world(c)))
}

fn side_normal(s: &Side) -> DVec3 {
    Plane::from_side(s).map_or(DVec3::ZERO, |p| p.normal)
}

pub(super) fn block_sides(bounds: Aabb, material: &str, uv_scale: f64, keep: DVec3) -> EditResult<(Vec<Side>, usize)> {
    let params = PrimitiveParams {
        kind: Primitive::Block,
        bounds,
        material: material.to_owned(),
        tex_scale: uv_scale,
        ..PrimitiveParams::default()
    };
    let mut next = 1u32;
    let mut sides = make_primitive(&params, &mut || {
        next += 1;
        next
    })?
    .into_iter()
    .next()
    .ok_or(EditError::Nothing("no block"))?;
    let mut face = None;
    for (i, s) in sides.iter_mut().enumerate() {
        if side_normal(s).dot(keep) > 0.99 {
            face = Some(i);
        } else {
            s.material = NODRAW.to_owned();
        }
    }
    let face = face.ok_or(EditError::Nothing("no face on the projection axis"))?;
    Ok((sides, face))
}

pub(super) fn add_pieces(doc: &mut Document, pieces: Vec<Piece<'_>>) -> EditResult<(Vec<Id>, Vec<FaceRef>)> {
    if pieces.is_empty() {
        return Err(EditError::Nothing("nothing to create"));
    }
    let ids = doc.create_solids(pieces.iter().map(|p| p.sides.clone()).collect(), Owner::World)?;
    let mut updates: Vec<(Id, Solid)> = Vec::new();
    let mut faces = Vec::new();
    for (id, piece) in ids.iter().zip(&pieces) {
        let Some(side) = piece.disp_side else { continue };
        let Some(node) = doc.scene.solids.get(id) else { continue };
        let Some(poly) = node.geom.face_for_side(side).map(|f| f.vertices.clone()) else { continue };
        let Some(s) = node.solid.sides.get(side) else { continue };
        let mut disp = create_disp_with_axes(&poly, piece.power, s.uaxis.axis, s.vaxis.axis)?;
        if let Some(shape) = &piece.shape
            && let Some(mesh) = build_disp(&poly, &disp)
        {
            shape(&mut disp, &mesh);
        }
        hp_geom::update_disp_tags(&mut disp, &poly);
        let mut solid = node.solid.clone();
        solid.sides[side].disp = Some(Box::new(disp));
        updates.push((*id, solid));
        faces.push(FaceRef::new(*id, side));
    }
    doc.update_solids(updates);
    Ok((ids, faces))
}

fn node_index(plan: &TilePlan, l: DVec3) -> (usize, usize) {
    let w = plan.width();
    let h = plan.ny * plan.cells() + 1;
    let gi = ((l.x - plan.lo_u) / plan.spacing).round().clamp(0.0, (w - 1) as f64) as usize;
    let gj = ((l.y - plan.lo_v) / plan.spacing).round().clamp(0.0, (h - 1) as f64) as usize;
    (gi, gj)
}

fn tile_piece<'a>(plan: &'a TilePlan, opts: &MeshDispOptions, i: usize, j: usize) -> EditResult<Piece<'a>> {
    let frame = plan.frame;
    let (u0, u1, v0, v1) = plan.tile_rect(i, j);
    let bounds = local_box(&frame, (u0, u1), (v0, v1), (plan.plane - plan.thickness, plan.plane));
    let (sides, top) = block_sides(bounds, &opts.material, opts.uv_scale, frame.up())?;
    let shape = move |d: &mut DispInfo, m: &DispMesh| {
        for (k, bp) in m.base_positions.iter().enumerate() {
            let l = frame.local(*bp);
            let (gi, gj) = node_index(plan, l);
            if let Some(slot) = d.distances.get_mut(k) {
                *slot = plan.node(gi, gj) - l.z;
            }
        }
    };
    Ok(Piece { sides, disp_side: Some(top), power: plan.power, shape: Some(Box::new(shape)) })
}

#[derive(Clone, Copy)]
enum Edge {
    MinU,
    MaxU,
    MinV,
    MaxV,
}

fn skirt_piece<'a>(
    plan: &'a TilePlan,
    opts: &MeshDispOptions,
    i: usize,
    j: usize,
    edge: Edge,
) -> EditResult<Piece<'a>> {
    let frame = plan.frame;
    let (u0, u1, v0, v1) = plan.tile_rect(i, j);
    let cells = plan.cells();
    let depth = plan.thickness.min(plan.tile);
    let (along0, outward, heights): (f64, DVec3, Vec<f64>) = match edge {
        Edge::MinU => {
            (v0, frame.world(DVec3::NEG_X), (0..=cells).map(|k| plan.node(i * cells, j * cells + k)).collect())
        }
        Edge::MaxU => {
            (v0, frame.world(DVec3::X), (0..=cells).map(|k| plan.node((i + 1) * cells, j * cells + k)).collect())
        }
        Edge::MinV => {
            (u0, frame.world(DVec3::NEG_Y), (0..=cells).map(|k| plan.node(i * cells + k, j * cells)).collect())
        }
        Edge::MaxV => {
            (u0, frame.world(DVec3::Y), (0..=cells).map(|k| plan.node(i * cells + k, (j + 1) * cells)).collect())
        }
    };
    let along_is_v = matches!(edge, Edge::MinU | Edge::MaxU);
    let top = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let (bu, bv) = match edge {
        Edge::MinU => ((u0, u0 + depth), (v0, v1)),
        Edge::MaxU => ((u1 - depth, u1), (v0, v1)),
        Edge::MinV => ((u0, u1), (v0, v0 + depth)),
        Edge::MaxV => ((u0, u1), (v1 - depth, v1)),
    };
    let bounds = local_box(&frame, bu, bv, (plan.base, top));
    let (sides, face) = block_sides(bounds, &opts.material, opts.uv_scale, outward)?;
    let (base, spacing) = (plan.base, plan.spacing);
    let shape = move |d: &mut DispInfo, m: &DispMesh| {
        for (k, bp) in m.base_positions.iter().enumerate() {
            let l = frame.local(*bp);
            let along = if along_is_v { l.y } else { l.x };
            let idx = (((along - along0) / spacing).round().max(0.0) as usize).min(heights.len() - 1);
            let fh = ((l.z - base) / (top - base)).clamp(0.0, 1.0);
            let target = DVec3::new(l.x, l.y, base + (heights[idx] - base) * fh);
            if let Some(slot) = d.offsets.get_mut(k) {
                *slot = frame.world(target) - *bp;
            }
        }
    };
    Ok(Piece { sides, disp_side: Some(face), power: plan.power, shape: Some(Box::new(shape)) })
}

fn bottom_pieces<'a>(plan: &TilePlan, opts: &MeshDispOptions) -> EditResult<Vec<Piece<'a>>> {
    let frame = plan.frame;
    let mut out = Vec::new();
    for j in 0..plan.ny {
        let mut i = 0;
        while i < plan.nx {
            if !plan.kept[j * plan.nx + i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < plan.nx && plan.kept[j * plan.nx + i] {
                i += 1;
            }
            let (u0, _, v0, v1) = plan.tile_rect(start, j);
            let (_, u1, _, _) = plan.tile_rect(i - 1, j);
            let bounds = local_box(&frame, (u0, u1), (v0, v1), (plan.base - plan.thickness, plan.base));
            let (sides, _) = block_sides(bounds, &opts.material, opts.uv_scale, -frame.up())?;
            out.push(Piece { sides, disp_side: None, power: plan.power, shape: None });
        }
    }
    Ok(out)
}

impl Document {
    pub fn create_mesh_displacements(&mut self, plan: &TilePlan, opts: &MeshDispOptions) -> EditResult<MeshDispResult> {
        check_options(opts)?;
        let mut pieces: Vec<Piece<'_>> = Vec::new();
        for j in 0..plan.ny {
            for i in 0..plan.nx {
                if plan.kept[j * plan.nx + i] {
                    pieces.push(tile_piece(plan, opts, i, j)?);
                }
            }
        }
        let tiles = pieces.len();
        if tiles == 0 {
            return Err(EditError::Nothing("no tile to create"));
        }
        let mut skirts = 0;
        if opts.skirt {
            for j in 0..plan.ny {
                for i in 0..plan.nx {
                    if !plan.kept[j * plan.nx + i] {
                        continue;
                    }
                    let (ii, jj) = (i as isize, j as isize);
                    for (di, dj, edge) in
                        [(-1, 0, Edge::MinU), (1, 0, Edge::MaxU), (0, -1, Edge::MinV), (0, 1, Edge::MaxV)]
                    {
                        if !plan.tile_kept(ii + di, jj + dj) {
                            pieces.push(skirt_piece(plan, opts, i, j, edge)?);
                            skirts += 1;
                        }
                    }
                }
            }
        }
        let mut bottoms = 0;
        if opts.bottom {
            let list = bottom_pieces(plan, opts)?;
            bottoms = list.len();
            pieces.extend(list);
        }
        let group = opts.group;
        self.transaction(UNDO_MESH_TO_DISP, |doc| {
            let (solids, faces) = add_pieces(doc, pieces)?;
            let mut group_id = None;
            if group && solids.len() > 1 {
                let objs: Vec<ObjRef> = solids.iter().map(|id| ObjRef::Solid(*id)).collect();
                group_id = Some(doc.group(&objs)?);
            }
            Ok::<_, EditError>(MeshDispResult { solids, faces, tiles, skirts, bottoms, group: group_id })
        })
    }
}
