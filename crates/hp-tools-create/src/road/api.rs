use hp_editor::{Document, Id, Owner};

use super::apply::{build_in_doc, list_roads, read_road, road_piece_of};
use super::build::{LateralSide, PieceRole, Road, RoadError, RoadPiece, RoadSample};
use super::model::RoadMeta;

#[derive(Clone, Debug)]
pub struct LoadedRoad {
    pub uid: u32,
    pub solid_ids: Vec<Id>,
    pub owner: Owner,
    pub road: Road,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadSummary {
    pub uid: u32,
    pub solid_ids: Vec<Id>,
    pub entity_id: Option<Id>,
    pub points: usize,
    pub lanes: u32,
    pub total_width: f64,
}

pub fn summarize(doc: &Document) -> Vec<RoadSummary> {
    list_roads(doc)
        .into_iter()
        .map(|e| RoadSummary {
            uid: e.meta.uid,
            entity_id: match e.record.owner {
                Owner::Entity(id) => Some(id),
                Owner::World => None,
            },
            solid_ids: e.record.ids,
            points: e.meta.points.len(),
            lanes: e.meta.lanes,
            total_width: e.meta.total_width(),
        })
        .collect()
}

pub fn load_road(doc: &Document, solid_or_entity: Id) -> Option<Result<LoadedRoad, RoadError>> {
    let piece = road_piece_of(doc, solid_or_entity)?;
    let (record, meta) = read_road(doc, piece)?;
    Some(build_in_doc(doc, &meta, &record.ids).map(|road| LoadedRoad {
        uid: meta.uid,
        solid_ids: record.ids,
        owner: record.owner,
        road,
    }))
}

pub fn load_roads(doc: &Document) -> Vec<LoadedRoad> {
    list_roads(doc)
        .into_iter()
        .filter_map(|e| {
            build_in_doc(doc, &e.meta, &e.record.ids).ok().map(|road| LoadedRoad {
                uid: e.meta.uid,
                solid_ids: e.record.ids,
                owner: e.record.owner,
                road,
            })
        })
        .collect()
}

impl Road {
    pub fn meta(&self) -> &RoadMeta {
        &self.meta
    }

    pub fn centerline(&self) -> &[RoadSample] {
        &self.samples
    }

    pub fn total_width(&self) -> f64 {
        self.half_width * 2.0
    }

    pub fn lane_area_width(&self) -> f64 {
        self.lane_half_width * 2.0
    }

    pub fn shoulder_width(&self) -> f64 {
        self.half_width - self.lane_half_width
    }

    pub fn lane_pieces(&self) -> Vec<&RoadPiece> {
        self.pieces_with(|r| matches!(r, PieceRole::Lane(_)))
    }

    pub fn shoulder_pieces(&self, side: LateralSide) -> Vec<&RoadPiece> {
        self.pieces_with(|r| *r == PieceRole::Shoulder(side))
    }

    pub fn line_pieces(&self) -> Vec<&RoadPiece> {
        self.pieces_with(|r| matches!(r, PieceRole::Line(_)))
    }

    pub fn flare_pieces(&self) -> Vec<&RoadPiece> {
        self.pieces_with(|r| matches!(r, PieceRole::Flare(_)))
    }

    pub fn surface_pieces(&self) -> Vec<&RoadPiece> {
        self.pieces.iter().collect()
    }
}
