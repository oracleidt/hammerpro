use std::collections::HashSet;

use hp_app::ViewKind;
use hp_editor::{Document, Id, ObjRef};
use hp_geom::roadside::{RoadsideError, Spine};

use crate::curve;
use crate::curve::meta::CurveMeta;
use crate::curve::section::{Profile, station_rolls};
use crate::road::api::load_road;
use crate::road::apply::{find_road, road_piece_of};
use crate::road::build::Road;

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Curve(CurveMeta),
    Road(u32),
}

impl Source {
    pub fn encode(&self) -> String {
        match self {
            Source::Curve(meta) => format!("c:{}", meta.encode().unwrap_or_default()),
            Source::Road(uid) => format!("r:{uid}"),
        }
    }

    pub fn decode(text: &str) -> Option<Self> {
        if let Some(rest) = text.strip_prefix("c:") {
            return CurveMeta::decode(rest).map(Source::Curve);
        }
        text.strip_prefix("r:")?.parse::<u32>().ok().map(Source::Road)
    }

    pub fn label(&self) -> String {
        match self {
            Source::Curve(meta) => format!("curve, {} points", meta.points.len()),
            Source::Road(uid) => format!("road {uid}"),
        }
    }
}

pub fn spine_of_curve(meta: &CurveMeta) -> Result<Spine, RoadsideError> {
    if meta.kind != ViewKind::Top {
        return Err(RoadsideError::BadValue("only a curve drawn in the Top view can guide the terrain"));
    }
    spine_of_curve_plane(meta)
}

pub fn spine_of_curve_plane(meta: &CurveMeta) -> Result<Spine, RoadsideError> {
    let width = meta.section.total_width(meta.thickness);
    let layout =
        curve::layout(&meta.points, meta.segments, width).map_err(|e| RoadsideError::Geometry(e.to_string()))?;
    let rolls = station_rolls(&layout.centers, &meta.section);
    let top = meta.depth.0.max(meta.depth.1);
    let n = layout.centers.len();
    let ditch = if meta.section.profile == Profile::Ditch { meta.section.clamped().ditch_depth } else { 0.0 };
    Spine::new(layout.centers, vec![top; n], rolls, width * 0.5, ditch).ok_or(RoadsideError::TooShort)
}

pub fn spine_of_road(road: &Road) -> Result<Spine, RoadsideError> {
    let stations = road.samples.iter().map(|s| s.center.truncate()).collect();
    let top = road.samples.iter().map(|s| s.center.z).collect();
    let roll = road.samples.iter().map(|s| s.roll).collect();
    Spine::new(stations, top, roll, road.half_width, 0.0).ok_or(RoadsideError::TooShort)
}

pub fn frame_of(source: &Source) -> ViewKind {
    match source {
        Source::Curve(meta) => meta.kind,
        Source::Road(_) => ViewKind::Top,
    }
}

pub fn resolve(doc: &Document, source: &Source) -> Result<Spine, RoadsideError> {
    match source {
        Source::Curve(meta) => spine_of_curve_plane(meta),
        Source::Road(uid) => {
            let entry = find_road(doc, *uid).ok_or(RoadsideError::BadValue("a road of the set no longer exists"))?;
            let piece = entry.record.ids.first().copied().ok_or(RoadsideError::Nothing)?;
            let loaded = load_road(doc, piece)
                .ok_or(RoadsideError::Nothing)?
                .map_err(|e| RoadsideError::Geometry(e.to_string()))?;
            spine_of_road(&loaded.road)
        }
    }
}

pub fn resolve_frame(doc: &Document, sources: &[Source]) -> Result<(Vec<Spine>, ViewKind), RoadsideError> {
    let first = sources.first().ok_or(RoadsideError::Nothing)?;
    let kind = frame_of(first);
    if sources.iter().any(|s| frame_of(s) != kind) {
        return Err(RoadsideError::BadValue(
            "the roads of one set must all be drawn in the same view: a curve of the Front or Side view cannot meet a road seen from above",
        ));
    }
    let spines = sources.iter().map(|s| resolve(doc, s)).collect::<Result<Vec<_>, _>>()?;
    Ok((spines, kind))
}

pub fn resolve_all(doc: &Document, sources: &[Source]) -> Result<Vec<Spine>, RoadsideError> {
    let (spines, kind) = resolve_frame(doc, sources)?;
    if kind != ViewKind::Top {
        return Err(RoadsideError::BadValue(
            "the terrain follows roads seen from above: a curve drawn in the Front or Side view cannot guide it",
        ));
    }
    Ok(spines)
}

fn pieces_of(doc: &Document, obj: ObjRef) -> Vec<Id> {
    match obj {
        ObjRef::Solid(id) => vec![id],
        ObjRef::Entity(e) => doc.scene().entity_solid_ids(e),
    }
}

pub fn source_of_piece(doc: &Document, piece: Id) -> Option<(Source, Vec<Id>)> {
    if road_piece_of(doc, piece).is_some() {
        let (record, meta) = crate::road::apply::read_road(doc, piece)?;
        return Some((Source::Road(meta.uid), record.ids));
    }
    doc.curve_tag(piece)?;
    let (record, meta) = curve::apply::read_curve(doc, piece)?;
    Some((Source::Curve(meta), record.ids))
}

pub fn sources_of_objects(doc: &Document, objects: &[ObjRef]) -> Vec<Source> {
    let mut seen: HashSet<Id> = HashSet::new();
    let mut out: Vec<Source> = Vec::new();
    for obj in objects.iter().take(256) {
        for piece in pieces_of(doc, *obj) {
            if seen.contains(&piece) {
                continue;
            }
            let Some((source, ids)) = source_of_piece(doc, piece) else { continue };
            seen.extend(ids);
            if !out.contains(&source) {
                out.push(source);
            }
        }
    }
    out.truncate(super::meta::MAX_SOURCES);
    out
}

pub fn sources_of_selection(doc: &Document) -> Vec<Source> {
    let objects: Vec<ObjRef> = doc.selection().objects.iter().copied().collect();
    sources_of_objects(doc, &objects)
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use super::*;
    use crate::curve::Section;

    fn meta(kind: ViewKind) -> CurveMeta {
        CurveMeta {
            kind,
            points: vec![DVec2::new(0.0, 0.0), DVec2::new(256.0, 0.0), DVec2::new(512.0, 128.0)],
            depth: (-8.0, 4.0),
            segments: 10,
            thickness: 96.0,
            tex_scale: 0.25,
            section: Section::default(),
        }
    }

    #[test]
    fn curve_spine_matches_the_road_layout() {
        let s = spine_of_curve(&meta(ViewKind::Top)).unwrap();
        assert_eq!(s.stations.len(), 11);
        assert_eq!(s.half_width, 48.0);
        assert!(s.top.iter().all(|z| *z == 4.0));
        assert_eq!(s.ditch, 0.0);
    }

    #[test]
    fn only_top_curves_are_roads() {
        assert!(spine_of_curve(&meta(ViewKind::Front)).is_err());
    }

    #[test]
    fn curb_profile_widens_the_road_and_ditch_dips() {
        let mut m = meta(ViewKind::Top);
        m.section.profile = Profile::Curb;
        m.section.curb_width = 10.0;
        assert_eq!(spine_of_curve(&m).unwrap().half_width, 58.0);
        m.section.profile = Profile::Ditch;
        m.section.ditch_depth = 5.0;
        assert_eq!(spine_of_curve(&m).unwrap().ditch, 5.0);
    }

    #[test]
    fn sources_round_trip() {
        let s = Source::Curve(meta(ViewKind::Top));
        assert_eq!(Source::decode(&s.encode()), Some(s));
        assert_eq!(Source::decode("r:12"), Some(Source::Road(12)));
        assert_eq!(Source::decode("x:12"), None);
        assert_eq!(Source::decode("r:abc"), None);
    }
}
