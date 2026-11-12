use std::collections::{HashMap, HashSet, VecDeque};

use glam::DVec3;
use hp_geom::{Aabb, Primitive, PrimitiveParams};

use super::vis::VisReport;
use super::{Finding, Impact, SolidIndex, Topic, fmt_point};
use crate::limits::areaportal::{AREAPORTAL_MATERIAL, fit_opening};
use crate::limits::portals::PortalFile;
use crate::scene::Owner;
use crate::selection::ObjRef;
use crate::{Document, EditError, EditResult, Id};

const PORTAL_CLASSES: [&str; 2] = ["func_areaportal", "func_areaportalwindow"];
const MAX_THICK: f64 = 16.0;
const MAX_OPENING: f64 = 512.0;
const MAX_GROUPS: usize = 400;

pub fn is_portal_class(class: &str) -> bool {
    PORTAL_CLASSES.iter().any(|c| class.eq_ignore_ascii_case(c))
}

pub fn is_door_class(class: &str) -> bool {
    let c = class.to_ascii_lowercase();
    c.starts_with("func_door") || c == "prop_door_rotating" || c == "func_movelinear"
}

fn thin_axis(b: &Aabb) -> usize {
    let s = b.size();
    if s.x <= s.y && s.x <= s.z {
        0
    } else if s.y <= s.z {
        1
    } else {
        2
    }
}

pub fn unsealed_edges(idx: &SolidIndex, b: &Aabb) -> usize {
    let k = thin_axis(b);
    let others: Vec<usize> = (0..3).filter(|a| *a != k).collect();
    let center = b.center();
    let mut open = 0;
    for (i, &a) in others.iter().enumerate() {
        let o = others[1 - i];
        for side in [b.min[a] - 2.0, b.max[a] + 2.0] {
            let sealed = (1..=3).all(|j| {
                let mut p = center;
                p[a] = side;
                p[o] = b.min[o] + (b.max[o] - b.min[o]) * f64::from(j) / 4.0;
                idx.contains(p).is_some()
            });
            if !sealed {
                open += 1;
            }
        }
    }
    open
}

fn portal_entities(doc: &Document) -> Vec<(Id, String, Aabb, usize)> {
    doc.scene
        .entities
        .values()
        .filter(|n| is_portal_class(&n.entity.classname) && doc.is_visible(ObjRef::Entity(n.entity.id)))
        .map(|n| {
            let id = n.entity.id;
            let solids = doc.scene.entity_solid_ids(id).len();
            (id, n.entity.classname.clone(), doc.scene.entity_solid_bounds(id).unwrap_or(Aabb::EMPTY), solids)
        })
        .collect()
}

fn names(doc: &Document) -> HashMap<String, Vec<Id>> {
    let mut out: HashMap<String, Vec<Id>> = HashMap::new();
    for n in doc.scene.entities.values() {
        if let Some(t) = n.entity.get("targetname").map(str::trim).filter(|t| !t.is_empty()) {
            out.entry(t.to_ascii_lowercase()).or_default().push(n.entity.id);
        }
    }
    out
}

fn door_bounds(doc: &Document, id: Id, model: &dyn Fn(&hp_vmf::Entity) -> Option<Aabb>) -> Option<Aabb> {
    let n = doc.scene.entity(id)?;
    if let Some(b) = doc.scene.entity_solid_bounds(id).filter(|b| !b.is_empty()) {
        return Some(b);
    }
    if let Some(b) = model(&n.entity).filter(|b| !b.is_empty()) {
        return Some(b);
    }
    let o = n.entity.origin()?;
    Some(Aabb::new(o - DVec3::new(28.0, 28.0, 56.0), o + DVec3::new(28.0, 28.0, 56.0)))
}

fn door_starts_open(e: &hp_vmf::Entity) -> bool {
    let num = |k: &str| e.get(k).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
    num("spawnpos") != 0 || (e.classname.to_ascii_lowercase().starts_with("func_door") && num("spawnflags") & 1 != 0)
}

pub fn check(doc: &Document, vis: Option<&VisReport>, model: &dyn Fn(&hp_vmf::Entity) -> Option<Aabb>) -> Vec<Finding> {
    let portals = portal_entities(doc);
    let mut out = Vec::new();
    if portals.is_empty() {
        return out;
    }
    let names = names(doc);
    let idx = SolidIndex::new(doc, true);
    let solid_owner: HashMap<Id, Id> =
        portals.iter().flat_map(|(e, ..)| doc.scene.entity_solid_ids(*e).into_iter().map(move |s| (s, *e))).collect();
    let mut useless: HashSet<Id> = HashSet::new();
    if let Some(v) = vis {
        for ap in &v.areaportals {
            if ap.areas.is_empty() && ap.entity != 0 {
                useless.insert(ap.entity);
            }
        }
        for line in &v.log_warnings {
            if let Some(rest) = line.strip_prefix("Brush ")
                && line.contains("doesn't touch two areas")
                && let Some(id) = rest.split(':').next().and_then(|n| n.trim().parse::<u32>().ok())
                && let Some(e) = solid_owner.get(&id)
            {
                useless.insert(*e);
            }
        }
        if v.areas <= 1 && v.clusters > 0 {
            out.push(
                Finding::new(
                    Topic::Areaportals,
                    "areaportals_no_areas",
                    Impact::High,
                    format!(
                        "The map has {} areaportals but only one area: the space goes around them (an opening without a \
                         portal, a gap in a wall, a window). Every route between two areas must be closed by world brushes \
                         or by an areaportal, otherwise no areaportal culls anything.",
                        portals.len()
                    ),
                )
                .score(1000.0)
                .objects(portals.iter().map(|p| ObjRef::Entity(p.0)).collect())
                .args(vec![portals.len().to_string()]),
            );
        }
    }
    for (id, class, b, solids) in &portals {
        let obj = vec![ObjRef::Entity(*id)];
        let at = b.center();
        let push =
            |out: &mut Vec<Finding>, code: &'static str, impact: Impact, msg: String, fix: Option<&'static str>| {
                let mut f = Finding::new(Topic::Areaportals, code, impact, msg)
                    .score(100.0)
                    .objects(obj.clone())
                    .points(vec![at])
                    .boxes(vec![*b])
                    .args(vec![id.to_string(), fmt_point(at)]);
                if let Some(x) = fix {
                    f = f.fix(x);
                }
                out.push(f);
            };
        if *solids == 0 || b.is_empty() {
            push(
                &mut out,
                "areaportal_empty",
                Impact::High,
                format!("Areaportal {id} has no brush: delete it."),
                Some("delete"),
            );
            continue;
        }
        if *solids > 1 {
            push(
                &mut out,
                "areaportal_brushes",
                Impact::High,
                format!(
                    "Areaportal {id} at {} has {solids} brushes: an areaportal must be exactly one brush.",
                    fmt_point(at)
                ),
                None,
            );
        }
        let size = b.size();
        let k = thin_axis(b);
        if size[k] > MAX_THICK {
            push(
                &mut out,
                "areaportal_thick",
                Impact::Low,
                format!(
                    "Areaportal {id} at {} is {:.0} units thick: make it a thin sheet (1 to 8 units) in the middle of the \
                     opening.",
                    fmt_point(at),
                    size[k]
                ),
                Some("refit"),
            );
        }
        let span = (0..3).filter(|a| *a != k).map(|a| size[a]).fold(0.0, f64::max);
        if span > MAX_OPENING {
            push(
                &mut out,
                "areaportal_large",
                Impact::Medium,
                format!(
                    "Areaportal {id} at {} closes a {span:.0} unit wide opening: areaportals work in narrow openings (door \
                     frames, windows, corridor ends). A wide one rarely culls; narrow the opening with world brushes.",
                    fmt_point(at)
                ),
                None,
            );
        }
        let open = unsealed_edges(&idx, b);
        if open > 0 {
            push(
                &mut out,
                "areaportal_not_sealed",
                Impact::High,
                format!(
                    "Areaportal {id} at {} does not touch the world on {open} of its 4 sides: the areas leak around it, so \
                     it separates nothing. Stretch it into the walls, floor and ceiling of the opening.",
                    fmt_point(at)
                ),
                Some("refit"),
            );
        }
        let touching: Vec<Id> = portals
            .iter()
            .filter(|(o, _, ob, _)| o != id && !ob.is_empty() && ob.intersects(&b.expand(1.0)))
            .map(|p| p.0)
            .collect();
        if let Some(other) = touching.first().filter(|o| **o > *id) {
            push(
                &mut out,
                "areaportal_touching",
                Impact::High,
                format!(
                    "Areaportals {id} and {other} touch: the tiny space between them becomes its own area and the compile \
                     warns that a portal touches more than two areas. Keep one, or leave real space between them."
                ),
                None,
            );
        }
        if useless.contains(id) {
            push(
                &mut out,
                "areaportal_useless",
                Impact::Medium,
                format!(
                    "Areaportal {id} at {} does not split two areas (the compile could not separate anything on its sides): \
                     it costs an entity and culls nothing. Delete it, or seal the opening it is in.",
                    fmt_point(at)
                ),
                Some("delete"),
            );
        }
        let Some(node) = doc.scene.entity(*id) else { continue };
        let e = &node.entity;
        let target = e.get("target").map(str::trim).filter(|t| !t.is_empty()).map(str::to_ascii_lowercase);
        if class.eq_ignore_ascii_case("func_areaportalwindow") {
            if target.as_ref().is_none_or(|t| !names.contains_key(t)) {
                push(
                    &mut out,
                    "areaportalwindow_target",
                    Impact::Medium,
                    format!(
                        "Areaportal window {id}: its target (the func_brush that fades in, usually a window pane) is missing."
                    ),
                    None,
                );
            }
            let fade = e.get("FadeDist").and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0);
            if fade <= 0.0 {
                push(
                    &mut out,
                    "areaportalwindow_fade",
                    Impact::Low,
                    format!("Areaportal window {id} has no FadeDist: it never closes, set FadeDist and FadeStartDist."),
                    None,
                );
            }
            continue;
        }
        match &target {
            Some(t) => match names.get(t) {
                None => push(
                    &mut out,
                    "areaportal_missing_door",
                    Impact::Medium,
                    format!(
                        "Areaportal {id} is linked to '{t}', which does not exist: the portal stays in its start state forever."
                    ),
                    Some("link_door"),
                ),
                Some(ids) => {
                    if let Some(door) =
                        ids.iter().filter_map(|d| doc.scene.entity(*d)).find(|d| is_door_class(&d.entity.classname))
                    {
                        let portal_open =
                            e.get("StartOpen").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(1) != 0;
                        if door_starts_open(&door.entity) != portal_open {
                            push(
                                &mut out,
                                "areaportal_door_state",
                                Impact::Medium,
                                format!(
                                    "Areaportal {id} starts {} but its door starts {}: set StartOpen to match the door.",
                                    if portal_open { "open" } else { "closed" },
                                    if portal_open { "closed" } else { "open" }
                                ),
                                Some("door_state"),
                            );
                        }
                    }
                }
            },
            None => {
                let door =
                    doc.scene.entities.values().filter(|d| is_door_class(&d.entity.classname)).find(|d| {
                        door_bounds(doc, d.entity.id, model).is_some_and(|db| db.intersects(&b.expand(16.0)))
                    });
                if let Some(d) = door {
                    push(
                        &mut out,
                        "areaportal_no_door_link",
                        Impact::Medium,
                        format!(
                            "Areaportal {id} sits in the frame of door {} but is not linked to it: link it (target) so the \
                             portal closes when the door closes.",
                            d.entity.id
                        ),
                        Some("link_door"),
                    );
                }
            }
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct PortalSuggestion {
    pub bounds: Aabb,
    pub door: Option<Id>,
    pub sides: (usize, usize),
    pub sealed: bool,
}

impl PortalSuggestion {
    pub fn finding(&self) -> Finding {
        let at = self.bounds.center();
        let small = self.sides.0.min(self.sides.1);
        let (code, msg) = match self.door {
            Some(d) => (
                "areaportal_door",
                format!(
                    "Put an areaportal in the frame of door {d} at {}: when the door is closed the engine stops drawing \
                     everything behind it ({} and {} visleaves on each side).",
                    fmt_point(at),
                    self.sides.0,
                    self.sides.1
                ),
            ),
            None => (
                "areaportal_opening",
                format!(
                    "Put an areaportal in the opening at {}: it is the only way between {} and {} visleaves, so it can \
                     separate two areas; the engine then only draws what is seen through the opening.",
                    fmt_point(at),
                    self.sides.0,
                    self.sides.1
                ),
            ),
        };
        let impact = if self.door.is_some() && small >= 2 || small >= 8 { Impact::Medium } else { Impact::Low };
        let mut f = Finding::new(Topic::Areaportals, code, impact, msg)
            .score(small as f64 * 30.0 + if self.door.is_some() { 200.0 } else { 0.0 })
            .points(vec![at])
            .boxes(vec![self.bounds])
            .args(vec![
                self.door.map_or_else(String::new, |d| d.to_string()),
                fmt_point(at),
                self.sides.0.to_string(),
                self.sides.1.to_string(),
            ]);
        if let Some(d) = self.door {
            f = f.objects(vec![ObjRef::Entity(d)]);
        }
        if self.sealed {
            f = f.fix("place_areaportal");
        }
        f
    }
}

fn covered(existing: &[Aabb], b: &Aabb) -> bool {
    existing.iter().any(|e| e.intersects(&b.expand(8.0)))
}

fn cluster_sides(portals: &PortalFile, removed: &HashSet<usize>, a: u32, b: u32) -> Option<(usize, usize)> {
    let n = portals.clusters as usize;
    let mut adj: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (i, p) in portals.portals.iter().enumerate() {
        if removed.contains(&i) {
            continue;
        }
        let [x, y] = p.clusters;
        if (x as usize) < n && (y as usize) < n {
            adj[x as usize].push(y);
            adj[y as usize].push(x);
        }
    }
    let flood = |start: u32| {
        let mut seen = vec![false; n];
        let mut q = VecDeque::from([start]);
        let mut count = 0;
        while let Some(c) = q.pop_front() {
            let ci = c as usize;
            if ci >= n || seen[ci] {
                continue;
            }
            seen[ci] = true;
            count += 1;
            q.extend(adj[ci].iter().copied());
        }
        (seen, count)
    };
    let (seen, ca) = flood(a);
    if seen.get(b as usize).copied().unwrap_or(true) {
        return None;
    }
    let (_, cb) = flood(b);
    Some((ca, cb))
}

pub fn suggest(
    doc: &Document,
    portals: Option<&PortalFile>,
    model: &dyn Fn(&hp_vmf::Entity) -> Option<Aabb>,
    include_open: bool,
) -> Vec<PortalSuggestion> {
    let idx = SolidIndex::new(doc, true);
    let mut existing: Vec<Aabb> = portal_entities(doc).into_iter().map(|p| p.2).filter(|b| !b.is_empty()).collect();
    let mut out: Vec<PortalSuggestion> = Vec::new();
    let mut doors: Vec<&crate::scene::EntityNode> =
        doc.scene.entities.values().filter(|n| is_door_class(&n.entity.classname)).collect();
    doors.sort_by_key(|n| n.entity.id);
    for d in doors {
        let Some(db) = door_bounds(doc, d.entity.id, model) else { continue };
        if covered(&existing, &db) {
            continue;
        }
        let Some(fit) = fit_opening(doc, db) else { continue };
        let span = fit.size();
        if span.max_element() > MAX_OPENING * 1.5 {
            continue;
        }
        let sealed = unsealed_edges(&idx, &fit) == 0;
        let sides = portals.and_then(|p| opening_cut(p, &fit)).unwrap_or((0, 0));
        existing.push(fit);
        out.push(PortalSuggestion { bounds: fit, door: Some(d.entity.id), sides, sealed });
    }
    if include_open && let Some(p) = portals {
        for (fit, sides) in openings(doc, &idx, p) {
            if covered(&existing, &fit) {
                continue;
            }
            existing.push(fit);
            out.push(PortalSuggestion { bounds: fit, door: None, sides, sealed: true });
        }
    }
    out
}

fn portal_plane(points: &[DVec3]) -> Option<(usize, f64, Aabb)> {
    if points.len() < 3 {
        return None;
    }
    let n = (points[1] - points[0]).cross(points[2] - points[0]).normalize_or_zero();
    let k = if n.x.abs() > 0.99 {
        0
    } else if n.y.abs() > 0.99 {
        1
    } else {
        return None;
    };
    let b = Aabb::from_points(points.iter().copied());
    Some((k, b.center()[k], b))
}

fn opening_cut(p: &PortalFile, fit: &Aabb) -> Option<(usize, usize)> {
    let k = thin_axis(fit);
    let at = fit.center()[k];
    let removed: HashSet<usize> = p
        .portals
        .iter()
        .enumerate()
        .filter(|(_, q)| {
            portal_plane(&q.points)
                .is_some_and(|(qk, qa, qb)| qk == k && (qa - at).abs() <= 8.0 && qb.intersects(&fit.expand(2.0)))
        })
        .map(|(i, _)| i)
        .collect();
    let first = removed.iter().next().map(|i| p.portals[*i].clusters)?;
    cluster_sides(p, &removed, first[0], first[1])
}

fn openings(doc: &Document, idx: &SolidIndex, p: &PortalFile) -> Vec<(Aabb, (usize, usize))> {
    let mut groups: Vec<(usize, f64, Aabb, Vec<usize>)> = Vec::new();
    for (i, q) in p.portals.iter().enumerate() {
        let Some((k, at, b)) = portal_plane(&q.points) else { continue };
        let u = 1 - k;
        let s = b.size();
        if s[u] < 24.0 || s.z < 48.0 || s[u] > 256.0 || s.z > 256.0 {
            continue;
        }
        if let Some(g) =
            groups.iter_mut().find(|g| g.0 == k && (g.1 - at).abs() <= 2.0 && g.2.expand(16.0).intersects(&b))
        {
            g.2 = g.2.union(&b);
            g.3.push(i);
        } else {
            groups.push((k, at, b, vec![i]));
        }
    }
    groups.retain(|g| {
        let s = g.2.size();
        s[1 - g.0] <= 256.0 && s.z <= 256.0
    });
    groups.truncate(MAX_GROUPS);
    let mut out = Vec::new();
    for (k, at, b, members) in groups {
        let removed: HashSet<usize> = members.iter().copied().collect();
        let first = p.portals[members[0]].clusters;
        let Some(sides) = cluster_sides(p, &removed, first[0], first[1]) else { continue };
        if sides.0.min(sides.1) < 2 {
            continue;
        }
        let mut probe = b;
        probe.min[k] = at - 1.0;
        probe.max[k] = at + 1.0;
        let Some(fit) = fit_opening(doc, probe) else { continue };
        if unsealed_edges(idx, &fit) != 0 || fit.size().max_element() > MAX_OPENING {
            continue;
        }
        out.push((fit, sides));
    }
    out.sort_by_key(|o| std::cmp::Reverse(o.1.0.min(o.1.1)));
    out
}

impl Document {
    pub fn place_areaportal(&mut self, bounds: Aabb, door: Option<Id>) -> EditResult<Id> {
        if let Some(d) = door {
            return self.seal_opening(d, bounds);
        }
        if bounds.is_empty() || bounds.size().min_element() <= 0.0 || !bounds.min.is_finite() || !bounds.max.is_finite()
        {
            return Err(EditError::Invalid("empty areaportal box".to_owned()));
        }
        let params = PrimitiveParams {
            kind: Primitive::Block,
            bounds,
            material: AREAPORTAL_MATERIAL.to_owned(),
            ..PrimitiveParams::default()
        };
        self.tx("Place areaportal", |doc| {
            let ids = doc.create_primitive(&params, Owner::World)?;
            let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Solid(*id)).collect();
            let kv = vec![("StartOpen".to_owned(), "1".to_owned()), ("PortalVersion".to_owned(), "1".to_owned())];
            doc.tie_to_entity(&objs, "func_areaportal", &kv)
        })
    }

    pub fn refit_areaportal(&mut self, portal: Id) -> EditResult<bool> {
        let Some(b) = self.scene.entity_solid_bounds(portal).filter(|b| !b.is_empty()) else {
            return Err(EditError::NotFound(ObjRef::Entity(portal)));
        };
        let Some(fit) = fit_opening(self, b) else { return Ok(false) };
        if unsealed_edges(&SolidIndex::new(self, true), &fit) != 0 {
            return Ok(false);
        }
        let old: Vec<ObjRef> = self.scene.entity_solid_ids(portal).into_iter().map(ObjRef::Solid).collect();
        let params = PrimitiveParams {
            kind: Primitive::Block,
            bounds: fit,
            material: AREAPORTAL_MATERIAL.to_owned(),
            ..PrimitiveParams::default()
        };
        self.tx("Fit areaportal", |doc| {
            doc.create_primitive(&params, Owner::Entity(portal))?;
            doc.delete(&old)?;
            Ok(true)
        })
    }

    pub fn link_areaportal_door(
        &mut self,
        portal: Id,
        model: &dyn Fn(&hp_vmf::Entity) -> Option<Aabb>,
    ) -> EditResult<bool> {
        let Some(b) = self.scene.entity_solid_bounds(portal).filter(|b| !b.is_empty()) else {
            return Err(EditError::NotFound(ObjRef::Entity(portal)));
        };
        let door = self
            .scene
            .entities
            .values()
            .filter(|d| is_door_class(&d.entity.classname))
            .find(|d| door_bounds(self, d.entity.id, model).is_some_and(|db| db.intersects(&b.expand(16.0))))
            .map(|d| d.entity.id);
        let Some(door) = door else { return Ok(false) };
        self.tx("Link areaportal", |doc| {
            doc.delete(&[ObjRef::Entity(portal)])?;
            doc.seal_opening(door, b)?;
            Ok(true)
        })
    }

    pub fn sync_areaportal_state(&mut self, portal: Id) -> EditResult<bool> {
        let Some(p) = self.scene.entity(portal) else { return Err(EditError::NotFound(ObjRef::Entity(portal))) };
        let Some(target) = p.entity.get("target").map(str::trim).filter(|t| !t.is_empty()).map(str::to_owned) else {
            return Ok(false);
        };
        let open = self
            .scene
            .entities
            .values()
            .find(|d| {
                is_door_class(&d.entity.classname)
                    && d.entity.get("targetname").is_some_and(|n| n.trim().eq_ignore_ascii_case(&target))
            })
            .map(|d| door_starts_open(&d.entity));
        let Some(open) = open else { return Ok(false) };
        let value = if open { "1" } else { "0" };
        if self.scene.entity(portal).and_then(|p| p.entity.get("StartOpen")) == Some(value) {
            return Ok(false);
        }
        self.tx("Fix areaportal", |doc| Ok(doc.update_entity(portal, |e| e.set("StartOpen", value))))
    }
}
