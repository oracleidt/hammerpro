use glam::DVec3;
use hp_geom::{
    Aabb, ClipPiece, Plane, carve_traced, clip_solid_traced, hollow_traced, merge_convex, merge_exact,
    rebuild_from_points, unique_vertices, validate_solid,
};
use hp_vmf::{Group, Side, Solid};

use crate::document::Document;
use crate::ids::{IdMap, IdSet, id_map, id_set};
use crate::scene::Owner;
use crate::selection::{ObjRef, SelectOp};
use crate::{EditError, EditResult, Id};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ClipKeep {
    Front,
    Back,
    #[default]
    Both,
}

const SIDE_LIST_KEYS: [&str; 2] = ["sides", "sides2"];

pub(crate) fn remap_side_list(value: &str, map: &IdMap<Vec<Id>>) -> Option<String> {
    let mut changed = false;
    let mut out: Vec<String> = Vec::new();
    for tok in value.split_ascii_whitespace() {
        match tok.parse::<Id>().ok().and_then(|id| map.get(&id)) {
            Some(new) => {
                changed = true;
                for n in new {
                    let n = n.to_string();
                    if !out.contains(&n) {
                        out.push(n);
                    }
                }
            }
            None => {
                if !out.iter().any(|o| o == tok) {
                    out.push(tok.to_owned());
                }
            }
        }
    }
    changed.then(|| out.join(" "))
}

pub(super) fn record_side_ids(original: &[Side], pieces: &[&ClipPiece], map: &mut IdMap<Vec<Id>>) {
    for (j, src) in original.iter().enumerate() {
        let new: Vec<Id> = pieces
            .iter()
            .flat_map(|p| p.sides.iter().zip(&p.origins))
            .filter(|(_, o)| **o == Some(j))
            .map(|(s, _)| s.id)
            .collect();
        if new != [src.id] {
            map.insert(src.id, new);
        }
    }
}

pub(super) fn derived_solid(template: &Solid, id: Id, sides: Vec<Side>) -> Solid {
    Solid {
        id,
        sides,
        editor: template.editor.clone(),
        hidden: template.hidden,
        extra_props: template.extra_props.clone(),
        extra_blocks: template.extra_blocks.clone(),
    }
}

impl Document {
    pub fn clip(&mut self, solids: &[Id], plane: &Plane, keep: ClipKeep) -> EditResult<Vec<Id>> {
        self.clip_with(solids, plane, keep, None)
    }

    pub fn clip_with(
        &mut self,
        solids: &[Id],
        plane: &Plane,
        keep: ClipKeep,
        cap_material: Option<&str>,
    ) -> EditResult<Vec<Id>> {
        let cap_material = cap_material.map(str::trim).filter(|m| !m.is_empty());
        let plane = Plane::new(plane.normal, plane.dist)
            .ok_or_else(|| EditError::Invalid("degenerate clip plane".to_owned()))?;
        let mut next = self.next_side_id;
        let mut gen_side = || {
            let id = next;
            next = next.saturating_add(1);
            id
        };
        let mut replace: Vec<(Id, Solid)> = Vec::new();
        let mut create: Vec<(Id, Owner, Vec<Side>)> = Vec::new();
        let mut side_ids: IdMap<Vec<Id>> = id_map(0);
        for id in solids {
            let Some(n) = self.scene.solids.get(id) else { continue };
            if !hp_geom::plane_splits(&n.geom, &plane) {
                continue;
            }
            let original = &n.solid.sides;
            let Ok((front, back)) = clip_solid_traced(original, &plane, &mut gen_side) else { continue };
            let (Some(mut front), Some(mut back)) = (front, back) else { continue };
            if let Some(material) = cap_material {
                for piece in [&mut front, &mut back] {
                    for (side, o) in piece.sides.iter_mut().zip(&piece.origins) {
                        if o.is_none() {
                            material.clone_into(&mut side.material);
                        }
                    }
                }
            }
            let mut s = n.solid.clone();
            match keep {
                ClipKeep::Front => {
                    for (side, o) in front.sides.iter_mut().zip(&front.origins) {
                        if let Some(src) = o.and_then(|o| original.get(o)) {
                            side.id = src.id;
                        }
                    }
                    record_side_ids(original, &[&front], &mut side_ids);
                    s.sides = front.sides;
                }
                ClipKeep::Back => {
                    record_side_ids(original, &[&back], &mut side_ids);
                    s.sides = back.sides;
                }
                ClipKeep::Both => {
                    record_side_ids(original, &[&back, &front], &mut side_ids);
                    s.sides = back.sides;
                    create.push((*id, n.owner, front.sides));
                }
            }
            replace.push((*id, s));
        }
        self.next_side_id = next;
        if replace.is_empty() {
            return Ok(Vec::new());
        }
        self.tx("Clip", |doc| {
            let mut result: Vec<Id> = replace.iter().map(|(id, _)| *id).collect();
            doc.update_solids(replace);
            doc.remap_side_refs(&side_ids);
            let mut new_solids = Vec::with_capacity(create.len());
            let mut new_selected = Vec::new();
            for (src, owner, sides) in create {
                let Some(template) = doc.scene.solids.get(&src).map(|n| n.solid.clone()) else { continue };
                let id = doc.alloc_object_id();
                if doc.selection.objects.contains(&ObjRef::Solid(src)) {
                    new_selected.push(ObjRef::Solid(id));
                }
                new_solids.push((derived_solid(&template, id, sides), owner));
            }
            result.extend(doc.add_solids(new_solids));
            doc.select(&new_selected, SelectOp::Add);
            Ok(result)
        })
    }

    pub(crate) fn remap_side_refs(&mut self, map: &IdMap<Vec<Id>>) {
        if map.is_empty() {
            return;
        }
        let updates: Vec<(Id, Vec<(String, String)>)> = self
            .scene
            .entities
            .values()
            .filter_map(|n| {
                let changes: Vec<(String, String)> = n
                    .entity
                    .props
                    .iter()
                    .filter(|(k, _)| SIDE_LIST_KEYS.iter().any(|s| k.eq_ignore_ascii_case(s)))
                    .filter_map(|(k, v)| remap_side_list(v, map).map(|nv| (k.clone(), nv)))
                    .collect();
                (!changes.is_empty()).then_some((n.entity.id, changes))
            })
            .collect();
        for (id, changes) in updates {
            self.update_entity(id, |e| {
                for (k, v) in changes {
                    e.set(&k, v);
                }
            });
        }
    }

    pub fn carve(&mut self, carvers: &[Id]) -> EditResult<usize> {
        let carver_set: IdSet = carvers.iter().copied().collect();
        let mut cutters: Vec<(Vec<Side>, Aabb)> = Vec::new();
        let mut targets = id_set(0);
        for id in carvers {
            let Some(n) = self.scene.solids.get(id) else { continue };
            if !n.geom.valid {
                continue;
            }
            cutters.push((n.solid.sides.clone(), n.geom.aabb));
            for t in self.solids_in_box(&n.geom.aabb) {
                targets.insert(t);
            }
        }
        if cutters.is_empty() {
            return Err(EditError::Nothing("no valid solid to carve with"));
        }
        let mut targets: Vec<(u64, Id)> = targets
            .into_iter()
            .filter(|t| !carver_set.contains(t))
            .filter_map(|t| {
                let n = self.scene.solids.get(&t)?;
                (n.owner == Owner::World && n.geom.valid && self.is_solid_visible(t)).then_some((n.order, t))
            })
            .collect();
        targets.sort_unstable();
        let mut next = self.next_side_id;
        let mut gen_side = || {
            let id = next;
            next = next.saturating_add(1);
            id
        };
        let mut results: Vec<(Id, Vec<ClipPiece>)> = Vec::new();
        let mut side_ids: IdMap<Vec<Id>> = id_map(0);
        for (_, t) in targets {
            let Some(n) = self.scene.solids.get(&t) else { continue };
            let tb = n.geom.aabb;
            let original = &n.solid.sides;
            let mut pieces = vec![ClipPiece::whole(original)];
            let mut touched = false;
            for (cs, cb) in &cutters {
                let overlap = tb.intersection(cb);
                if overlap.is_empty() || overlap.size().min_element() <= hp_geom::PLANE_EPS {
                    continue;
                }
                let mut next_pieces = Vec::with_capacity(pieces.len() + 4);
                for p in pieces {
                    match carve_traced(&p.sides, cs, &mut gen_side) {
                        Ok(v) => {
                            if v.len() != 1 || v.first().map(|q| &q.sides) != Some(&p.sides) {
                                touched = true;
                            }
                            next_pieces.extend(v.into_iter().map(|q| q.compose(&p.origins)));
                        }
                        Err(_) => next_pieces.push(p),
                    }
                }
                pieces = next_pieces;
            }
            if touched {
                let refs: Vec<&ClipPiece> = pieces.iter().collect();
                record_side_ids(original, &refs, &mut side_ids);
                results.push((t, pieces));
            }
        }
        self.next_side_id = next;
        if results.is_empty() {
            return Ok(0);
        }
        let count = results.len();
        self.tx("Carve", |doc| {
            let mut new_solids = Vec::new();
            let mut removed = Vec::with_capacity(results.len());
            for (t, pieces) in results {
                let Some(template) = doc.scene.solids.get(&t).map(|n| n.solid.clone()) else { continue };
                for piece in pieces {
                    let id = doc.alloc_object_id();
                    new_solids.push((derived_solid(&template, id, piece.sides), Owner::World));
                }
                removed.push(t);
            }
            let groups = doc.groups_of(&removed.iter().copied().collect(), &id_set(0));
            doc.remove_solids(&removed);
            doc.add_solids(new_solids);
            doc.remap_side_refs(&side_ids);
            doc.cleanup_empty_groups(groups);
            doc.deselect_missing();
            Ok(count)
        })
    }

    pub fn hollow(&mut self, solids: &[Id], thickness: f64) -> EditResult<Vec<Id>> {
        let mut next = self.next_side_id;
        let mut gen_side = || {
            let id = next;
            next = next.saturating_add(1);
            id
        };
        let mut plans: Vec<(Id, Vec<Vec<Side>>)> = Vec::new();
        let mut side_ids: IdMap<Vec<Id>> = id_map(0);
        for id in solids {
            let Some(n) = self.scene.solids.get(id) else { continue };
            let walls = hollow_traced(&n.solid.sides, thickness, &mut gen_side)?;
            if !walls.is_empty() {
                let refs: Vec<&ClipPiece> = walls.iter().collect();
                record_side_ids(&n.solid.sides, &refs, &mut side_ids);
                plans.push((*id, walls.into_iter().map(|w| w.sides).collect()));
            }
        }
        self.next_side_id = next;
        if plans.is_empty() {
            return Err(EditError::Nothing("no solid to hollow"));
        }
        self.tx("Make hollow", |doc| {
            let mut all_new = Vec::new();
            let mut select = Vec::new();
            for (src, walls) in plans {
                let Some(n) = doc.scene.solids.get(&src) else { continue };
                let template = n.solid.clone();
                let owner = n.owner;
                let mut editor_group = template.editor.group_id;
                if owner == Owner::World {
                    let gid = doc.alloc_object_id();
                    let mut g = Group { id: gid, ..Group::default() };
                    g.editor.group_id = editor_group;
                    g.editor.color = template.editor.color;
                    doc.put_group(g);
                    editor_group = Some(gid);
                }
                let mut new_solids = Vec::with_capacity(walls.len());
                for sides in walls {
                    let id = doc.alloc_object_id();
                    let mut s = derived_solid(&template, id, sides);
                    s.editor.group_id = editor_group;
                    new_solids.push((s, owner));
                }
                doc.remove_solids(&[src]);
                let ids = doc.add_solids(new_solids);
                if owner == Owner::World {
                    select.extend(ids.iter().map(|id| ObjRef::Solid(*id)));
                } else if let Owner::Entity(e) = owner {
                    select.push(ObjRef::Entity(e));
                }
                all_new.extend(ids);
            }
            doc.remap_side_refs(&side_ids);
            doc.deselect_missing();
            doc.select(&select, SelectOp::Add);
            Ok(all_new)
        })
    }

    pub const MERGE_NOT_CONVEX: &'static str = "the brushes do not form a convex shape";
    pub const MERGE_MIXED_OWNERS: &'static str = "the brushes do not belong to the same entity";

    pub fn merge_solids(&mut self, solids: &[Id]) -> EditResult<Id> {
        self.merge_impl(solids, false)
    }

    pub fn merge_brushes(&mut self, solids: &[Id]) -> EditResult<Id> {
        self.merge_impl(solids, true)
    }

    fn merge_impl(&mut self, solids: &[Id], exact: bool) -> EditResult<Id> {
        let mut unique = id_set(solids.len());
        for id in solids {
            if self.scene.solids.contains_key(id) {
                unique.insert(*id);
            }
        }
        if unique.len() < 2 {
            return Err(EditError::Nothing("select at least two solids to merge"));
        }
        let mut owners = unique.iter().filter_map(|id| self.scene.solids.get(id).map(|n| n.owner));
        let first_owner = owners.next();
        if exact && owners.any(|o| Some(o) != first_owner) {
            return Err(EditError::Invalid(Self::MERGE_MIXED_OWNERS.to_owned()));
        }
        let lists: Vec<Vec<Side>> =
            unique.iter().filter_map(|id| self.scene.solids.get(id).map(|n| n.solid.sides.clone())).collect();
        let mut next = self.next_side_id;
        let mut gen_side = || {
            let id = next;
            next = next.saturating_add(1);
            id
        };
        let merged = if exact {
            merge_exact(&lists, &mut gen_side)?.ok_or_else(|| EditError::Invalid(Self::MERGE_NOT_CONVEX.to_owned()))?
        } else {
            merge_convex(&lists, &mut gen_side)?
        };
        validate_solid(&merged)?;
        self.next_side_id = next;
        let first = *unique.first().ok_or(EditError::Nothing("no solid"))?;
        let others: Vec<ObjRef> = unique.iter().skip(1).map(|id| ObjRef::Solid(*id)).collect();
        let mut solid =
            self.scene.solids.get(&first).map(|n| n.solid.clone()).ok_or(EditError::NotFound(ObjRef::Solid(first)))?;
        let source_refs: Vec<&[Side]> = lists.iter().map(Vec::as_slice).collect();
        let side_ids = super::boolean::plane_side_map(&source_refs, &merged);
        solid.sides = merged;
        self.tx("Merge solids", |doc| {
            doc.update_solid(first, solid);
            doc.remap_side_refs(&side_ids);
            doc.delete(&others)?;
            Ok(first)
        })
    }

    pub fn solid_vertices(&self, id: Id) -> Option<(Vec<DVec3>, Vec<Vec<usize>>)> {
        let n = self.scene.solids.get(&id)?;
        Some(unique_vertices(&n.geom))
    }

    pub fn set_solid_vertices(&mut self, id: Id, points: &[DVec3]) -> EditResult<()> {
        let n = self.scene.solids.get(&id).ok_or(EditError::NotFound(ObjRef::Solid(id)))?;
        if points.iter().any(|p| !p.is_finite()) {
            return Err(EditError::Invalid("vertex is not finite".to_owned()));
        }
        let mut next = self.next_side_id;
        let sides = rebuild_from_points(points, &n.solid.sides, &mut || {
            let id = next;
            next = next.saturating_add(1);
            id
        })?;
        validate_solid(&sides)?;
        let mut solid = n.solid.clone();
        solid.sides = sides;
        self.next_side_id = next;
        self.tx("Edit vertices", |doc| {
            doc.update_solid(id, solid);
            doc.deselect_missing();
            Ok(())
        })
    }
}

const NODRAW_MATERIAL: &str = "TOOLS/TOOLSNODRAW";

const OPAQUE_TOOLS: [&str; 4] = ["tools/toolsnodraw", "tools/toolsblack", "tools/toolsskybox", "tools/toolsskybox2d"];

const SEE_THROUGH_HINTS: [&str; 8] =
    ["glass", "water", "window", "fence", "grate", "chainlink", "translucent", "alpha"];

fn may_be_see_through(material: &str) -> bool {
    let m = material.trim().to_ascii_lowercase().replace('\\', "/");
    let m = m.strip_prefix("materials/").unwrap_or(&m);
    if m.starts_with("tools/") {
        return !OPAQUE_TOOLS.contains(&m);
    }
    SEE_THROUGH_HINTS.iter().any(|h| m.contains(h))
}

impl Document {
    pub fn hidden_face_scan(&self) -> (Vec<Id>, Vec<hp_geom::CoverageSolid>) {
        let mut ids = Vec::new();
        let mut solids = Vec::new();
        for n in self.scene.solids.values() {
            let id = n.solid.id;
            if !n.geom.valid || !self.is_solid_visible(id) {
                continue;
            }
            let world = n.owner == Owner::World;
            let detail = match n.owner {
                Owner::Entity(e) => self
                    .scene
                    .entities
                    .get(&e)
                    .is_some_and(|en| en.entity.classname.eq_ignore_ascii_case("func_detail")),
                Owner::World => false,
            };
            let covers = world && n.solid.sides.iter().all(|s| s.disp.is_none() && !may_be_see_through(&s.material));
            let candidates: Vec<bool> = n
                .geom
                .faces
                .iter()
                .map(|f| {
                    (world || detail)
                        && n.solid.sides.get(f.side_index).is_some_and(|s| {
                            s.disp.is_none() && !s.material.trim().to_ascii_lowercase().starts_with("tools/")
                        })
                })
                .collect();
            if !covers && !candidates.contains(&true) {
                continue;
            }
            ids.push(id);
            solids.push(hp_geom::CoverageSolid { geom: n.geom.clone(), candidates, covers });
        }
        (ids, solids)
    }

    pub fn find_hidden_faces(&self) -> Vec<crate::FaceRef> {
        let (ids, solids) = self.hidden_face_scan();
        hp_geom::hidden_faces(&solids)
            .into_iter()
            .filter_map(|h| {
                let side = solids.get(h.solid)?.geom.faces.get(h.face)?.side_index;
                Some(crate::FaceRef { solid: *ids.get(h.solid)?, side })
            })
            .collect()
    }

    pub fn apply_nodraw(&mut self, faces: &[crate::FaceRef]) -> EditResult<usize> {
        let mut per_solid: IdMap<Vec<usize>> = id_map(faces.len());
        for f in faces {
            if self.scene.solids.get(&f.solid).is_some_and(|n| f.side < n.solid.sides.len()) {
                per_solid.entry(f.solid).or_default().push(f.side);
            }
        }
        let mut count = 0;
        let mut updates: Vec<(Id, Solid)> = Vec::with_capacity(per_solid.len());
        for (id, sides) in &per_solid {
            let Some(n) = self.scene.solids.get(id) else { continue };
            let mut s = n.solid.clone();
            for i in sides {
                if let Some(side) = s.sides.get_mut(*i)
                    && !side.material.eq_ignore_ascii_case(NODRAW_MATERIAL)
                {
                    NODRAW_MATERIAL.clone_into(&mut side.material);
                    count += 1;
                }
            }
            if s != n.solid {
                updates.push((*id, s));
            }
        }
        if updates.is_empty() {
            return Err(EditError::Nothing("no face to turn into nodraw"));
        }
        self.tx("Nodraw hidden faces", |doc| {
            doc.update_solids(updates);
            Ok(count)
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use glam::DVec3;
    use hp_geom::Plane;
    use hp_vmf::{Entity, Map, Solid};

    use crate::Document;
    use crate::tests::{cube, vmf};

    fn solid_with_ids(id: u32, first: u32, min: DVec3, size: f64) -> Solid {
        let mut sides = cube(min, size);
        for (k, s) in sides.iter_mut().enumerate() {
            s.id = first + k as u32;
        }
        Solid { id, sides, ..Solid::default() }
    }

    fn face_list_entity(id: u32, class: &str, sides: &str) -> Entity {
        let mut e = Entity { id, classname: class.into(), ..Entity::default() };
        e.set("sides", sides);
        e.set("origin", "32 32 64");
        e
    }

    fn list(doc: &Document, id: u32) -> Vec<u32> {
        let v = doc.scene().entity(id).expect("entity").entity.get("sides").unwrap_or_default().to_owned();
        v.split_ascii_whitespace().map(|t| t.parse().expect("id")).collect()
    }

    fn faces_on(doc: &Document, plane: &Plane) -> Vec<u32> {
        let mut out = Vec::new();
        for n in doc.scene().solids.values() {
            for s in &n.solid.sides {
                if Plane::from_side(s).is_some_and(|p| p.approx_eq(plane, 1e-9, 1e-6)) {
                    out.push(s.id);
                }
            }
        }
        out.sort_unstable();
        out
    }

    fn sorted(mut v: Vec<u32>) -> Vec<u32> {
        v.sort_unstable();
        v
    }

    #[test]
    fn merging_brushes_only_when_convex() {
        use crate::{EditError, Owner};
        let mut doc = Document::new_empty();
        let a = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("a");
        let b = doc.create_solid(cube(DVec3::new(64.0, 0.0, 0.0), 64.0), Owner::World).expect("b");
        let c = doc.create_solid(cube(DVec3::new(64.0, 64.0, 0.0), 64.0), Owner::World).expect("c");
        doc.clear_history();
        let before = vmf(&doc);
        let err = doc.merge_brushes(&[a, b, c]).expect_err("not convex");
        assert!(matches!(&err, EditError::Invalid(s) if s == Document::MERGE_NOT_CONVEX), "{err:?}");
        assert_eq!(vmf(&doc), before);
        assert_eq!(doc.history_len().0, 0);
        let m = crate::tests::check_undo_redo(&mut doc, |d| d.merge_brushes(&[a, b]).expect("merge"));
        assert_eq!(m, a);
        assert!(doc.scene().solid(b).is_none());
        let g = &doc.scene().solid(a).expect("merged").geom;
        assert_eq!(g.faces.len(), 6);
        assert!((g.volume() - 128.0 * 64.0 * 64.0).abs() < 1e-6);
        assert!(doc.merge_solids(&[a, c]).is_ok());
        let mut doc = Document::new_empty();
        let a = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("a");
        let e =
            doc.create_brush_entity("func_detail", &[], vec![cube(DVec3::new(64.0, 0.0, 0.0), 64.0)]).expect("entity");
        let b = doc.scene().entity_solid_ids(e)[0];
        let err = doc.merge_brushes(&[a, b]).expect_err("owners");
        assert!(matches!(&err, EditError::Invalid(s) if s == Document::MERGE_MIXED_OWNERS), "{err:?}");
        let b2 = doc.create_solid(cube(DVec3::new(64.0, 64.0, 0.0), 64.0), Owner::Entity(e)).expect("b2");
        doc.merge_brushes(&[b, b2]).expect("same entity");
        assert_eq!(doc.scene().entity_solid_ids(e), vec![b]);
        assert!(doc.merge_brushes(&[b]).is_err());
    }

    #[test]
    fn nodraw_on_hidden_faces() {
        use crate::tests::box_sides;
        use crate::{FaceRef, Owner};
        let mut doc = Document::new_empty();
        let wall = doc
            .create_solid(
                box_sides(DVec3::new(-16.0, -256.0, 0.0), DVec3::new(0.0, 256.0, 256.0), "BRICK/WALL"),
                Owner::World,
            )
            .expect("wall");
        let boxed = doc
            .create_solid(
                box_sides(DVec3::new(0.0, -32.0, 0.0), DVec3::new(64.0, 32.0, 64.0), "WOOD/BOX"),
                Owner::World,
            )
            .expect("box");
        let detail = doc
            .create_brush_entity(
                "func_detail",
                &[],
                vec![box_sides(DVec3::new(0.0, 64.0, 0.0), DVec3::new(32.0, 96.0, 32.0), "METAL/PIPE")],
            )
            .expect("detail");
        let detail_solid = doc.scene().entity_solid_ids(detail)[0];
        let glass = doc
            .create_solid(
                box_sides(DVec3::new(-32.0, 128.0, 0.0), DVec3::new(-16.0, 160.0, 64.0), "GLASS/WINDOW01"),
                Owner::World,
            )
            .expect("glass");
        doc.clear_history();
        let mut found = doc.find_hidden_faces();
        found.sort_by_key(|f| (f.solid, f.side));
        let west = |doc: &Document, id: u32| {
            let n = doc.scene().solid(id).expect("solid");
            let i = n.solid.sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal == DVec3::NEG_X));
            FaceRef { solid: id, side: i.expect("west face") }
        };
        let east_of_glass = {
            let n = doc.scene().solid(glass).expect("glass");
            let i = n.solid.sides.iter().position(|s| Plane::from_side(s).is_some_and(|p| p.normal == DVec3::X));
            FaceRef { solid: glass, side: i.expect("east face") }
        };
        assert!(found.contains(&west(&doc, boxed)), "{found:?}");
        assert!(found.contains(&west(&doc, detail_solid)), "{found:?}");
        assert!(found.contains(&east_of_glass));
        assert!(!found.iter().any(|f| f.solid == wall), "{found:?}");
        let n = crate::tests::check_undo_redo(&mut doc, |d| d.apply_nodraw(&found).expect("apply"));
        assert_eq!(n, found.len());
        assert_eq!(
            doc.scene().solid(boxed).expect("box").solid.sides[west(&doc, boxed).side].material,
            "TOOLS/TOOLSNODRAW"
        );
        assert!(doc.find_hidden_faces().is_empty());
        assert!(doc.apply_nodraw(&found).is_err(), "already nodraw");
        assert!(super::may_be_see_through("tools/toolsclip") && !super::may_be_see_through("TOOLS/TOOLSNODRAW"));
        assert!(super::may_be_see_through("Glass/Window01") && !super::may_be_see_through("brick/wall"));
    }

    #[test]
    fn clipping_with_the_current_material_on_the_cut_faces() {
        use crate::{ClipKeep, Owner};
        let mut doc = Document::new_empty();
        let id = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("cube");
        let plane = Plane::new(DVec3::X, 32.0).expect("plane");
        let ids = doc.clip_with(&[id], &plane, ClipKeep::Both, Some("TOOLS/TOOLSNODRAW")).expect("clip");
        assert_eq!(ids.len(), 2);
        for piece in &ids {
            let sides = &doc.scene().solid(*piece).expect("piece").solid.sides;
            let on_cut = |s: &&hp_vmf::Side| {
                Plane::from_side(s)
                    .is_some_and(|p| (p.normal.x.abs() - 1.0).abs() < 1e-9 && (p.dist.abs() - 32.0).abs() < 1e-9)
            };
            let caps: Vec<&str> = sides.iter().filter(on_cut).map(|s| s.material.as_str()).collect();
            assert_eq!(caps, vec!["TOOLS/TOOLSNODRAW"]);
            assert!(sides.iter().filter(|s| !on_cut(s)).all(|s| s.material == "DEV/DEV_MEASUREGENERIC01"));
        }
        let mut doc = Document::new_empty();
        let id = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("cube");
        doc.clip_with(&[id], &plane, ClipKeep::Back, Some("  ")).expect("clip");
        let sides = &doc.scene().solid(id).expect("piece").solid.sides;
        assert!(sides.iter().all(|s| s.material == "DEV/DEV_MEASUREGENERIC01"));
    }

    #[test]
    fn carving_updates_overlay_and_cubemap_faces() {
        let mut map = Map::new_empty();
        map.world.solids.push(solid_with_ids(2, 10, DVec3::ZERO, 64.0));
        map.world.solids.push(solid_with_ids(3, 20, DVec3::new(16.0, 16.0, 48.0), 32.0));
        map.entities.push(face_list_entity(50, "info_overlay", "10 11"));
        map.entities.push(face_list_entity(51, "env_cubemap", "10"));
        map.entities.push(face_list_entity(52, "info_overlay", "20"));
        let mut doc = Document::from_map(map);
        let before = vmf(&doc);
        assert_eq!(doc.carve(&[3]).expect("carve"), 1);
        let top = Plane::new(DVec3::Z, 64.0).expect("plane");
        let bottom = Plane::new(DVec3::NEG_Z, 0.0).expect("plane");
        let tops = faces_on(&doc, &top);
        assert!(tops.len() >= 2, "the top face is split: {tops:?}");
        assert!(!tops.contains(&10), "pieces get new ids");
        let bottoms = faces_on(&doc, &bottom);
        assert_eq!(sorted(list(&doc, 50)), sorted([tops.clone(), bottoms].concat()));
        assert_eq!(sorted(list(&doc, 51)), tops);
        assert_eq!(list(&doc, 52), vec![20]);
        assert!(doc.undo());
        assert_eq!(vmf(&doc), before, "undo restores the face lists too");
        assert!(doc.redo());
        assert_eq!(sorted(list(&doc, 51)), tops);
    }

    #[test]
    fn carving_away_a_face_removes_it_from_lists() {
        let mut map = Map::new_empty();
        map.world.solids.push(solid_with_ids(2, 10, DVec3::ZERO, 64.0));
        map.world.solids.push(solid_with_ids(3, 20, DVec3::new(32.0, -16.0, -16.0), 128.0));
        map.entities.push(face_list_entity(50, "info_overlay", "14 15"));
        let mut doc = Document::from_map(map);
        doc.carve(&[3]).expect("carve");
        let west = Plane::new(DVec3::NEG_X, 0.0).expect("plane");
        assert_eq!(list(&doc, 50), faces_on(&doc, &west));
        assert_eq!(list(&doc, 50).len(), 1);
    }

    #[test]
    fn hollowing_moves_face_lists_to_the_walls() {
        for thickness in [8.0, -8.0] {
            let mut map = Map::new_empty();
            map.world.solids.push(solid_with_ids(2, 10, DVec3::ZERO, 64.0));
            map.entities.push(face_list_entity(50, "info_overlay", "10"));
            map.entities.push(face_list_entity(51, "env_cubemap", "11 14"));
            let mut doc = Document::from_map(map);
            let before = vmf(&doc);
            doc.hollow(&[2], thickness).expect("hollow");
            let out = if thickness > 0.0 { 0.0 } else { 8.0 };
            let top = Plane::new(DVec3::Z, 64.0 + out).expect("plane");
            let bottom = Plane::new(DVec3::NEG_Z, out).expect("plane");
            let east = Plane::new(DVec3::X, 64.0 + out).expect("plane");
            let tops = faces_on(&doc, &top);
            assert!(!tops.is_empty());
            assert_eq!(sorted(list(&doc, 50)), tops, "thickness {thickness}");
            assert_eq!(sorted(list(&doc, 51)), sorted([faces_on(&doc, &bottom), faces_on(&doc, &east)].concat()));
            assert!(doc.undo());
            assert_eq!(vmf(&doc), before);
        }
    }
}
