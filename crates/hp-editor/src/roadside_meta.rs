use hp_vmf::{EditorInfo, Solid};

use crate::{Document, EditError, EditResult, Id, ObjRef, Owner};

pub const ROADSIDE_KEY: &str = "hammerpro_roadside";

const DATA_PREFIX: &str = "hammerpro_roadside_";
const CHUNK: usize = 700;
const FULL_COPY_LIMIT: usize = 2100;
const NEIGHBOUR_GAP: f64 = 512.0;
const MAX_SCAN: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub struct RoadsideRecord {
    pub ids: Vec<Id>,
    pub data: String,
    pub owner: Owner,
}

fn is_roadside_key(k: &str) -> bool {
    k.len() >= ROADSIDE_KEY.len() && k.get(..ROADSIDE_KEY.len()).is_some_and(|p| p.eq_ignore_ascii_case(ROADSIDE_KEY))
}

pub fn editor_roadside_tag(editor: &EditorInfo) -> Option<&str> {
    editor
        .extra_props
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(ROADSIDE_KEY))
        .map(|(_, v)| v.trim())
        .filter(|v| !v.is_empty())
}

fn editor_roadside_data(editor: &EditorInfo) -> String {
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

fn chunks(data: &str) -> Vec<String> {
    data.as_bytes().chunks(CHUNK).map(|c| String::from_utf8_lossy(c).into_owned()).collect()
}

fn tagged(solid: &Solid, tag: &str, data: Option<&[String]>) -> Solid {
    let mut s = solid.clone();
    s.editor.extra_props.retain(|(k, _)| !is_roadside_key(k));
    s.editor.extra_props.push((ROADSIDE_KEY.to_owned(), tag.to_owned()));
    if let Some(list) = data {
        for (i, c) in list.iter().enumerate() {
            s.editor.extra_props.push((format!("{DATA_PREFIX}{i}"), c.clone()));
        }
    }
    s
}

impl Document {
    pub fn roadside_tag(&self, id: Id) -> Option<&str> {
        self.scene().solid(id).and_then(|n| editor_roadside_tag(&n.solid.editor))
    }

    pub fn selected_roadside_piece(&self) -> Option<Id> {
        let sel = self.selection();
        for obj in sel.objects.iter().take(MAX_SCAN) {
            match *obj {
                ObjRef::Solid(id) => {
                    if self.roadside_tag(id).is_some() {
                        return Some(id);
                    }
                }
                ObjRef::Entity(e) => {
                    if let Some(id) =
                        self.scene().entity_solid_ids(e).into_iter().find(|id| self.roadside_tag(*id).is_some())
                    {
                        return Some(id);
                    }
                }
            }
        }
        None
    }

    pub fn roadside_record(&self, piece: Id) -> Option<RoadsideRecord> {
        let scene = self.scene();
        let start = scene.solid(piece)?;
        let tag = editor_roadside_tag(&start.solid.editor)?;
        let owner = start.owner;
        let mut pool: Vec<(Id, hp_geom::Aabb)> = scene
            .solids()
            .iter()
            .filter(|(id, n)| **id != piece && n.owner == owner && editor_roadside_tag(&n.solid.editor) == Some(tag))
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
            .map(|n| editor_roadside_data(&n.solid.editor))
            .find(|d| !d.is_empty())?;
        Some(RoadsideRecord { ids, data, owner })
    }

    pub fn tag_roadside_pieces(&mut self, pieces: &[Id], data: &str) -> EditResult<()> {
        let Some(first) = pieces.iter().copied().min() else {
            return Err(EditError::Nothing("no roadside pieces to tag"));
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
            return Err(EditError::Nothing("no roadside pieces to tag"));
        }
        self.tx("Tag roadside", |doc| {
            doc.update_solids(updates);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_is_reassembled_in_order() {
        let mut e = EditorInfo::default();
        e.extra_props.push((format!("{DATA_PREFIX}10"), "c".to_owned()));
        e.extra_props.push((format!("{DATA_PREFIX}2"), "b".to_owned()));
        e.extra_props.push((format!("{DATA_PREFIX}0"), "a".to_owned()));
        e.extra_props.push((ROADSIDE_KEY.to_owned(), "7".to_owned()));
        e.extra_props.push(("hammerpro_curve".to_owned(), "9".to_owned()));
        assert_eq!(editor_roadside_data(&e), "abc");
        assert_eq!(editor_roadside_tag(&e), Some("7"));
        let t = tagged(&Solid::default(), "3", None);
        assert_eq!(editor_roadside_tag(&t.editor), Some("3"));
    }

    #[test]
    fn long_data_is_split_in_chunks() {
        let data = "a".repeat(1500) + "end";
        let list = chunks(&data);
        assert_eq!(list.len(), 3);
        assert_eq!(list.concat(), data);
    }
}
