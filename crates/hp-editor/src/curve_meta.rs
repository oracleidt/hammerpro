use hp_vmf::{EditorInfo, Solid};

use crate::{Document, EditError, EditResult, Id, ObjRef, Owner};

pub const CURVE_KEY: &str = "hammerpro_curve";

const DATA_PREFIX: &str = "hammerpro_curve_";
const CHUNK: usize = 700;
const FULL_COPY_LIMIT: usize = 2100;
const NEIGHBOUR_GAP: f64 = 0.5;
const MAX_SCAN: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct CurveRecord {
    pub ids: Vec<Id>,
    pub data: String,
    pub owner: Owner,
}

fn is_curve_key(k: &str) -> bool {
    k.len() >= CURVE_KEY.len() && k.get(..CURVE_KEY.len()).is_some_and(|p| p.eq_ignore_ascii_case(CURVE_KEY))
}

pub fn editor_curve_tag(editor: &EditorInfo) -> Option<&str> {
    editor
        .extra_props
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(CURVE_KEY))
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

fn editor_curve_data(editor: &EditorInfo) -> String {
    let mut parts: Vec<(usize, &str)> = editor
        .extra_props
        .iter()
        .filter_map(|(k, v)| {
            let head = k.get(..DATA_PREFIX.len())?;
            if !head.eq_ignore_ascii_case(DATA_PREFIX) {
                return None;
            }
            let n = k.get(DATA_PREFIX.len()..)?.parse::<usize>().ok()?;
            Some((n, v.as_str()))
        })
        .collect();
    parts.sort_unstable_by_key(|(n, _)| *n);
    parts.into_iter().map(|(_, v)| v).collect()
}

pub(crate) fn prepare_road_contents(map: &mut hp_vmf::Map, scene: &crate::Scene) {
    let mut roads = std::collections::HashSet::new();
    for solid in map.world.solids.iter().chain(map.entities.iter().flat_map(|entity| &entity.solids)) {
        let Some(tag) = editor_curve_tag(&solid.editor) else { continue };
        if !roads.contains(tag) && editor_curve_data(&solid.editor).starts_with("r1;uid=") {
            roads.insert(tag.to_owned());
        }
    }
    if roads.is_empty() {
        return;
    }
    for solid in &mut map.world.solids {
        if !editor_curve_tag(&solid.editor).is_some_and(|tag| roads.contains(tag)) {
            continue;
        }
        for (index, side) in solid.sides.iter_mut().enumerate() {
            let anchor = scene
                .solid(solid.id)
                .and_then(|node| node.geom.face_for_side(index))
                .map(|f| hp_geom::polygon_center(&f.vertices));
            stabilize_road_plane(side, anchor);
            if !side.extra_props.iter().any(|(key, _)| key.eq_ignore_ascii_case("contents")) {
                side.add_compile_contents(hp_vmf::CONTENTS_DETAIL);
            }
        }
    }
    for solid in map.entities.iter_mut().flat_map(|entity| &mut entity.solids) {
        if editor_curve_tag(&solid.editor).is_some_and(|tag| roads.contains(tag)) {
            for (index, side) in solid.sides.iter_mut().enumerate() {
                let anchor = scene
                    .solid(solid.id)
                    .and_then(|node| node.geom.face_for_side(index))
                    .map(|f| hp_geom::polygon_center(&f.vertices));
                stabilize_road_plane(side, anchor);
            }
        }
    }
}

fn stabilize_road_plane(side: &mut hp_vmf::Side, anchor: Option<glam::DVec3>) {
    let normal = side.normal();
    let dist = side.plane_distance();
    if side.disp.is_some() || !normal.is_finite() || normal.length_squared() < 0.5 || !dist.is_finite() {
        return;
    }
    let anchor = anchor.unwrap_or_else(|| side.plane.iter().sum::<glam::DVec3>() / 3.0);
    let plane = hp_geom::Plane { normal, dist }.to_points_at(anchor);
    if plane.map(|p| p.as_vec3()) != side.plane.map(|p| p.as_vec3()) {
        side.plane = plane;
    }
}

fn strip_curve_keys(editor: &mut EditorInfo) {
    editor.extra_props.retain(|(k, _)| !is_curve_key(k));
}

fn chunks(data: &str) -> Vec<String> {
    let bytes = data.as_bytes();
    bytes.chunks(CHUNK).map(|c| String::from_utf8_lossy(c).into_owned()).collect()
}

fn tagged(solid: &Solid, tag: &str, data: Option<&[String]>) -> Solid {
    let mut s = solid.clone();
    strip_curve_keys(&mut s.editor);
    s.editor.extra_props.push((CURVE_KEY.to_owned(), tag.to_owned()));
    if let Some(list) = data {
        for (i, c) in list.iter().enumerate() {
            s.editor.extra_props.push((format!("{DATA_PREFIX}{i}"), c.clone()));
        }
    }
    s
}

impl Document {
    pub fn curve_tag(&self, id: Id) -> Option<&str> {
        self.scene().solid(id).and_then(|n| editor_curve_tag(&n.solid.editor))
    }

    pub fn selected_curve_piece(&self) -> Option<Id> {
        let sel = self.selection();
        for obj in sel.objects.iter().take(MAX_SCAN) {
            match *obj {
                ObjRef::Solid(id) => {
                    if self.curve_tag(id).is_some() {
                        return Some(id);
                    }
                }
                ObjRef::Entity(e) => {
                    if let Some(id) =
                        self.scene().entity_solid_ids(e).into_iter().find(|id| self.curve_tag(*id).is_some())
                    {
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    pub fn curve_record(&self, piece: Id) -> Option<CurveRecord> {
        let scene = self.scene();
        let start = scene.solid(piece)?;
        let tag = editor_curve_tag(&start.solid.editor)?;
        let owner = start.owner;
        let mut pool: Vec<(Id, hp_geom::Aabb)> = scene
            .solids()
            .iter()
            .filter(|(id, n)| **id != piece && n.owner == owner && editor_curve_tag(&n.solid.editor) == Some(tag))
            .map(|(id, n)| (*id, n.bounds))
            .collect();
        let mut ids = vec![piece];
        let mut queue = vec![start.bounds];
        while let Some(b) = queue.pop() {
            let grown = b.expand(NEIGHBOUR_GAP);
            let mut i = 0;
            while i < pool.len() {
                if grown.intersects(&pool[i].1) {
                    let (id, bounds) = pool.swap_remove(i);
                    ids.push(id);
                    queue.push(bounds);
                } else {
                    i += 1;
                }
            }
        }
        ids.sort_unstable();
        let data = ids
            .iter()
            .filter_map(|id| scene.solid(*id))
            .map(|n| editor_curve_data(&n.solid.editor))
            .find(|d| !d.is_empty())?;
        Some(CurveRecord { ids, data, owner })
    }

    pub fn tag_curve_pieces(&mut self, pieces: &[Id], data: &str) -> EditResult<()> {
        let Some(first) = pieces.iter().copied().min() else {
            return Err(EditError::Nothing("no curve pieces to tag"));
        };
        let tag = first.to_string();
        let list = chunks(data);
        let everywhere = data.len() <= FULL_COPY_LIMIT;
        let last = pieces.iter().copied().max();
        let updates: Vec<(Id, Solid)> = pieces
            .iter()
            .filter_map(|id| {
                let node = self.scene().solid(*id)?;
                let holder = everywhere || *id == first || Some(*id) == last;
                Some((*id, tagged(&node.solid, &tag, holder.then_some(list.as_slice()))))
            })
            .collect();
        if updates.is_empty() {
            return Err(EditError::Nothing("no curve pieces to tag"));
        }
        self.tx("Tag curve", |doc| {
            doc.update_solids(updates);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exporting_legacy_roads_marks_chunkless_pieces_without_changing_the_document() {
        let mut map = hp_vmf::Map::new_empty();
        for id in 2..7 {
            let mut sides = crate::tests::box_sides(
                glam::DVec3::new((id * 128) as f64, 0.0, 0.0),
                glam::DVec3::new((id * 128 + 64) as f64, 64.0, 8.0),
                "dev/dev_measuregeneric01b",
            );
            for (index, side) in sides.iter_mut().enumerate() {
                side.id = 100 + id * 10 + index as u32;
            }
            let mut solid = Solid { id, sides, ..Default::default() };
            solid.editor.extra_props.push((CURVE_KEY.into(), if id < 5 { "road" } else { "curve" }.into()));
            if id == 2 {
                solid.editor.extra_props.push((format!("{DATA_PREFIX}0"), "r1;uid=1;".into()));
            }
            if id == 5 {
                solid.editor.extra_props.push((format!("{DATA_PREFIX}0"), "c1;".into()));
            }
            if id == 4 {
                for side in &mut solid.sides {
                    side.extra_props.push(("CONTENTS".into(), "0".into()));
                }
            }
            map.world.solids.push(solid);
        }
        let doc = Document::from_map(map);
        let before = hp_vmf::write(&doc.scene().to_map());
        let saved = doc.to_map();
        assert_eq!(saved.world.solids.len(), 5);
        assert!(saved.entities.is_empty());
        for solid in &saved.world.solids {
            let expected = if solid.id < 4 { hp_vmf::CONTENTS_DETAIL } else { 0 };
            assert!(solid.sides.iter().all(|side| side.compile_contents() == expected));
        }
        assert_eq!(hp_vmf::write(&doc.scene().to_map()), before);
        let text = hp_vmf::write(&saved);
        let reloaded = Document::from_map(hp_vmf::parse(&text).unwrap());
        assert_eq!(hp_vmf::write(&reloaded.to_map()), text);
    }

    #[test]
    fn chunks_rejoin() {
        let data = "a".repeat(2000) + "end";
        let list = chunks(&data);
        assert_eq!(list.len(), 3);
        assert_eq!(list.concat(), data);
    }

    #[test]
    fn data_is_reassembled_in_order() {
        let mut e = EditorInfo::default();
        e.extra_props.push((format!("{DATA_PREFIX}10"), "c".to_owned()));
        e.extra_props.push((format!("{DATA_PREFIX}2"), "b".to_owned()));
        e.extra_props.push((format!("{DATA_PREFIX}0"), "a".to_owned()));
        e.extra_props.push((CURVE_KEY.to_owned(), "7".to_owned()));
        assert_eq!(editor_curve_data(&e), "abc");
        assert_eq!(editor_curve_tag(&e), Some("7"));
        strip_curve_keys(&mut e);
        assert!(e.extra_props.is_empty());
    }
}
