use glam::{DVec2, DVec3};

use super::build::{Piece, add_pieces, block_sides, local_box};
use super::{AxisChoice, Frame, MeshDispError, PowerChoice, check_material};
use crate::document::Document;
use crate::selection::{FaceRef, ObjRef};
use crate::{EditError, EditResult, Id, Owner};

const UNDO_TILE_SELECTION: &str = "Tile displacements from selection";
const FLAT_TOLERANCE: f64 = 0.5;
const AXIS_DOT: f64 = 0.9999;
const OVERLAP_EPS: f64 = 0.01;

#[derive(Clone, Debug, PartialEq)]
pub struct TileSelectionOptions {
    pub axis: AxisChoice,
    pub flip: bool,
    pub tile_size: f64,
    pub power: PowerChoice,
    pub material: String,
    pub uv_scale: f64,
    pub thickness: f64,
    pub delete_source: bool,
    pub group: bool,
}

impl Default for TileSelectionOptions {
    fn default() -> Self {
        Self {
            axis: AxisChoice::Z,
            flip: false,
            tile_size: 256.0,
            power: PowerChoice::Auto,
            material: "DEV/DEV_MEASUREGENERIC01B".to_owned(),
            uv_scale: 0.25,
            thickness: 16.0,
            delete_source: true,
            group: true,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileSelectionResult {
    pub solids: Vec<Id>,
    pub faces: Vec<FaceRef>,
    pub tiles: usize,
    pub sources: usize,
    pub removed: usize,
    pub skipped_faces: usize,
}

struct Source {
    solid: Id,
    normal: DVec3,
    poly: Vec<DVec3>,
}

fn polygon_area(poly: &[DVec3], normal: DVec3) -> f64 {
    let mut sum = DVec3::ZERO;
    for i in 0..poly.len() {
        sum += poly[i].cross(poly[(i + 1) % poly.len()]);
    }
    (sum.dot(normal) * 0.5).abs()
}

fn overlaps(poly: &[DVec2], rect: (f64, f64, f64, f64)) -> bool {
    let (u0, u1, v0, v1) = rect;
    let corners = [DVec2::new(u0, v0), DVec2::new(u1, v0), DVec2::new(u0, v1), DVec2::new(u1, v1)];
    let span = |axis: DVec2, pts: &[DVec2]| {
        pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.dot(axis)), hi.max(p.dot(axis))))
    };
    let mut axes = vec![DVec2::X, DVec2::Y];
    for i in 0..poly.len() {
        let e = poly[(i + 1) % poly.len()] - poly[i];
        if let Some(n) = DVec2::new(e.y, -e.x).try_normalize() {
            axes.push(n);
        }
    }
    axes.into_iter().all(|axis| {
        let (pl, ph) = span(axis, poly);
        let (rl, rh) = span(axis, &corners);
        rh > pl + OVERLAP_EPS && rl < ph - OVERLAP_EPS
    })
}

impl Document {
    fn tile_sources(&self) -> Vec<Source> {
        let mut faces: Vec<FaceRef> = self.selected_faces();
        if faces.is_empty() {
            let mut solids: Vec<Id> = Vec::new();
            for o in self.selected_objects() {
                match o {
                    ObjRef::Solid(id) => solids.push(id),
                    ObjRef::Entity(id) => solids.extend(self.scene().entity_solid_ids(id)),
                }
            }
            for id in solids {
                if let Some(n) = self.scene.solids.get(&id) {
                    faces.extend((0..n.solid.sides.len()).map(|side| FaceRef::new(id, side)));
                }
            }
        }
        let mut out = Vec::new();
        for f in faces {
            let Some(n) = self.scene.solids.get(&f.solid) else { continue };
            if n.solid.sides.get(f.side).is_none_or(|s| s.disp.is_some()) {
                continue;
            }
            if let Some(g) = n.geom.face_for_side(f.side) {
                out.push(Source { solid: f.solid, normal: g.plane.normal, poly: g.vertices.clone() });
            }
        }
        out
    }

    pub fn tile_selection(&mut self, opts: &TileSelectionOptions) -> EditResult<TileSelectionResult> {
        let bad = |m: &str| Err(EditError::from(MeshDispError::Option(m.to_owned())));
        if !opts.tile_size.is_finite() || !(super::MIN_TILE..=super::MAX_TILE).contains(&opts.tile_size) {
            return bad("tile size must be between 16 and 4096");
        }
        if !opts.thickness.is_finite() || !(1.0..=4096.0).contains(&opts.thickness) {
            return bad("thickness must be between 1 and 4096");
        }
        if !opts.uv_scale.is_finite() || !(0.001..=64.0).contains(&opts.uv_scale) {
            return bad("texture scale must be between 0.001 and 64");
        }
        check_material(&opts.material)?;
        let sources = self.tile_sources();
        if sources.is_empty() {
            return Err(EditError::Nothing("select brushes or faces to tile"));
        }
        let (axis, flip) = match opts.axis {
            AxisChoice::Z => (2, opts.flip),
            AxisChoice::X => (0, opts.flip),
            AxisChoice::Y => (1, opts.flip),
            AxisChoice::Best => {
                let mut area = DVec3::ZERO;
                let mut signed = DVec3::ZERO;
                for s in &sources {
                    let a = polygon_area(&s.poly, s.normal);
                    area += s.normal.abs() * a;
                    signed += s.normal * a;
                }
                let axis = if area.x >= area.y && area.x >= area.z {
                    0
                } else if area.y >= area.z {
                    1
                } else {
                    2
                };
                (axis, (signed[axis] < 0.0) ^ opts.flip)
            }
        };
        let frame = Frame::new(axis, flip);
        let power = match opts.power {
            PowerChoice::Fixed(p) => p,
            PowerChoice::Auto => 3,
        };
        let mut pieces: Vec<Piece<'_>> = Vec::new();
        let mut used: Vec<Id> = Vec::new();
        let mut skipped = 0;
        for s in &sources {
            let local: Vec<DVec3> = s.poly.iter().map(|p| frame.local(*p)).collect();
            let (zmin, zmax) =
                local.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.z), b.max(p.z)));
            if s.normal.dot(frame.up()) < AXIS_DOT || zmax - zmin > FLAT_TOLERANCE {
                skipped += 1;
                continue;
            }
            let flat: Vec<DVec2> = local.iter().map(|p| DVec2::new(p.x, p.y)).collect();
            let (u0, u1) = flat.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.x), b.max(p.x)));
            let (v0, v1) = flat.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p.y), b.max(p.y)));
            let count = |lo: f64, hi: f64| (((hi - lo) / opts.tile_size - 1e-6).ceil().max(1.0)) as usize;
            let (nu, nv) = (count(u0, u1), count(v0, v1));
            if pieces.len() + nu * nv > super::MAX_TILES {
                return Err(MeshDispError::TooManyTiles(pieces.len() + nu * nv, super::MAX_TILES).into());
            }
            let (cu, cv) = ((u1 - u0) / nu as f64, (v1 - v0) / nv as f64);
            let height = zmax;
            let before = pieces.len();
            for j in 0..nv {
                for i in 0..nu {
                    let rect =
                        (u0 + i as f64 * cu, u0 + (i + 1) as f64 * cu, v0 + j as f64 * cv, v0 + (j + 1) as f64 * cv);
                    if !overlaps(&flat, rect) {
                        continue;
                    }
                    let bounds =
                        local_box(&frame, (rect.0, rect.1), (rect.2, rect.3), (height - opts.thickness, height));
                    let (sides, top) = block_sides(bounds, &opts.material, opts.uv_scale, frame.up())?;
                    pieces.push(Piece { sides, disp_side: Some(top), power, shape: None });
                }
            }
            if pieces.len() > before && !used.contains(&s.solid) {
                used.push(s.solid);
            }
        }
        if pieces.is_empty() {
            return Err(EditError::Nothing("no face of the selection faces the projection axis"));
        }
        let tiles = pieces.len();
        let remove: Vec<ObjRef> = if opts.delete_source {
            used.iter()
                .filter(|id| self.scene.solids.get(*id).is_some_and(|n| n.owner == Owner::World))
                .map(|id| ObjRef::Solid(*id))
                .collect()
        } else {
            Vec::new()
        };
        let group = opts.group;
        self.transaction(UNDO_TILE_SELECTION, |doc| {
            let (solids, faces) = add_pieces(doc, pieces)?;
            let mut removed = 0;
            if !remove.is_empty() {
                removed = doc.delete(&remove)?;
            }
            if group && solids.len() > 1 {
                let objs: Vec<ObjRef> = solids.iter().map(|id| ObjRef::Solid(*id)).collect();
                doc.group(&objs)?;
            }
            Ok::<_, EditError>(TileSelectionResult {
                solids,
                faces,
                tiles,
                sources: used.len(),
                removed,
                skipped_faces: skipped,
            })
        })
    }
}
