#![forbid(unsafe_code)]

mod entities;
mod extras;
mod lumps;
pub mod pak;
mod solids;
pub mod textures;

use std::collections::{HashMap, HashSet};
use std::path::Path;

use glam::DVec3;
use hp_bsp::Bsp;
use hp_vmf::{Entity, Id, Map};
use rayon::prelude::*;

use crate::lumps::{Lumps, contents};
use crate::solids::{BuiltSolid, IdGen};

pub const LIMITS: &[&str] = &[
    "func_instance contents are merged into the map",
    "visgroups, groups, hidden objects and comments are not in compiled maps",
    "hint, skip and origin brushes are removed by the compiler",
    "brush faces keep their textures, but texture locks and smoothing groups are lost",
];

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DecompileError {
    #[error("{0}")]
    Bsp(#[from] hp_bsp::BspError),
    #[error("the compiled map holds no brushes")]
    Empty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toggles {
    pub brushes: bool,
    pub displacements: bool,
    pub entities: bool,
    pub brush_entities: bool,
    pub static_props: bool,
    pub overlays: bool,
    pub cubemaps: bool,
    pub areaportals: bool,
    pub occluders: bool,
    pub group_details: bool,
}

impl Default for Toggles {
    fn default() -> Self {
        Self {
            brushes: true,
            displacements: true,
            entities: true,
            brush_entities: true,
            static_props: true,
            overlays: true,
            cubemaps: true,
            areaportals: true,
            occluders: true,
            group_details: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub map_name: String,
    pub known_outputs: Option<HashSet<String>>,
    pub toggles: Toggles,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub version: i32,
    pub brushes: usize,
    pub world_solids: usize,
    pub detail_solids: usize,
    pub detail_entities: usize,
    pub entity_solids: usize,
    pub failed_brushes: usize,
    pub empty_brushes: usize,
    pub displacements: usize,
    pub failed_displacements: usize,
    pub static_props: usize,
    pub overlays: usize,
    pub overlays_without_sides: usize,
    pub cubemaps: usize,
    pub occluders: usize,
    pub failed_occluders: usize,
    pub areaportals: usize,
    pub entities: usize,
    pub orphan_models: usize,
    pub visclusters: usize,
    pub unused_brushes: usize,
    pub empty_brush_entities: usize,
    pub renamed_materials: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Decompiled {
    pub map: Map,
    pub report: Report,
}

pub fn decompile_file(path: &Path, options: &Options) -> Result<Decompiled, DecompileError> {
    let bsp = Bsp::open(path)?;
    let mut options = options.clone();
    if options.map_name.is_empty() {
        options.map_name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    }
    decompile(&bsp, &options)
}

fn brush_model(e: &hp_bsp::Entity) -> Option<usize> {
    e.get("model")?.strip_prefix('*')?.trim().parse().ok()
}

fn origin_of(e: &hp_bsp::Entity) -> DVec3 {
    e.origin().map_or(DVec3::ZERO, |o| DVec3::new(o[0], o[1], o[2]))
}

pub fn decompile(bsp: &Bsp, options: &Options) -> Result<Decompiled, DecompileError> {
    let mut report = Report { version: bsp.version(), ..Report::default() };
    let l = Lumps::read(bsp, &mut report.warnings)?;
    report.brushes = l.brushes.len();
    if l.brushes.is_empty() {
        return Err(DecompileError::Empty);
    }
    let map_name = options.map_name.as_str();
    let mut source_entities = bsp.entities().unwrap_or_else(|e| {
        report.warnings.push(format!("the entity lump could not be read: {e}"));
        Vec::new()
    });
    report.entities = source_entities.len();

    let mut owner: Vec<Option<usize>> = vec![None; l.brushes.len()];
    for m in (1..l.models.len()).chain(std::iter::once(0)) {
        for b in solids::model_brushes(&l, m) {
            owner[b].get_or_insert(m);
        }
    }
    let mut model_offsets: HashMap<usize, DVec3> = HashMap::new();
    for e in &source_entities {
        if let Some(m) = brush_model(e)
            && m > 0
        {
            model_offsets.entry(m).or_insert_with(|| origin_of(e));
        }
    }

    for (m, entity, origin) in solids::orphan_models(&l, &model_offsets) {
        model_offsets.insert(m, origin);
        source_entities.push(entity);
        report.orphan_models += 1;
    }

    let toggles = options.toggles;
    let built: Vec<Option<BuiltSolid>> = (0..l.brushes.len())
        .into_par_iter()
        .map(|b| {
            if !(toggles.brushes || toggles.brush_entities && owner[b].is_some_and(|m| m > 0)) {
                return None;
            }
            let offset = owner[b].and_then(|m| model_offsets.get(&m)).copied().unwrap_or_default();
            solids::build_brush(&l, b, offset, map_name)
        })
        .collect();
    let disps: Vec<Option<extras::DispSolid>> = (0..l.dispinfo.len())
        .into_par_iter()
        .map(|i| if toggles.displacements { extras::displacement(&l, i, map_name) } else { None })
        .collect();

    let max_hammer_id = source_entities
        .iter()
        .filter_map(|e| e.get("hammerid").and_then(|v| v.trim().parse::<Id>().ok()))
        .max()
        .unwrap_or(1);
    let mut ids = IdGen { next_object: max_hammer_id.saturating_add(1).max(2), next_side: 1 };
    let mut used_ids: HashSet<Id> = HashSet::from([1]);
    let mut sides = extras::SideIndex::new();

    let mut world_solids = Vec::new();
    let mut detail = Vec::new();
    let mut portals = Vec::new();
    let mut visclusters = Vec::new();
    let owned_any = owner.iter().any(Option::is_some);
    let mut model_solids: HashMap<usize, Vec<hp_vmf::Solid>> = HashMap::new();
    for (b, built) in built.into_iter().enumerate() {
        let Some(mut built) = built else {
            if !(toggles.brushes || toggles.brush_entities && owner[b].is_some_and(|m| m > 0)) {
                continue;
            }
            if l.brushes[b].numsides <= 0 {
                report.empty_brushes += 1;
            } else {
                report.failed_brushes += 1;
            }
            continue;
        };
        built.assign_ids(&mut ids);
        for rec in &built.sides {
            sides.add(rec);
        }
        report.renamed_materials += built.renamed_materials;
        let brush_contents = l.brushes[b].contents;
        match owner[b] {
            Some(m) if m > 0 && model_offsets.contains_key(&m) => {
                model_solids.entry(m).or_default().push(built.solid);
            }
            _ if brush_contents & contents::AREAPORTAL != 0 => portals.push(built.solid),
            None if owned_any => {
                if built.solid.sides.iter().all(|s| s.material.eq_ignore_ascii_case("tools/toolstrigger")) {
                    visclusters.push(built.solid);
                } else {
                    report.unused_brushes += 1;
                }
            }
            _ if brush_contents & contents::DETAIL != 0 => detail.push(built.solid),
            _ => world_solids.push(built.solid),
        }
    }
    for d in disps {
        let Some(mut d) = d else {
            if toggles.displacements {
                report.failed_displacements += 1;
            }
            continue;
        };
        d.built.assign_ids(&mut ids);
        for (k, rec) in d.built.sides.iter().enumerate() {
            if k == d.side {
                sides.add_face(d.face, rec.id);
            }
            sides.add(rec);
        }
        report.renamed_materials += d.built.renamed_materials;
        report.displacements += 1;
        world_solids.push(d.built.solid);
    }

    let mut world = Entity { id: 1, classname: "worldspawn".to_owned(), ..Entity::default() };
    let mut entities: Vec<Entity> = Vec::with_capacity(source_entities.len());
    let mut portal_entities: Vec<usize> = Vec::new();
    let mut occluder_entities: Vec<(usize, usize)> = Vec::new();
    for src in &source_entities {
        if src.classname().eq_ignore_ascii_case("worldspawn") {
            let converted = entities::convert(src, 1, &mut HashSet::new(), options.known_outputs.as_ref());
            world.props = converted
                .props
                .into_iter()
                .filter(|(k, _)| !["world_mins", "world_maxs", "mapversion"].iter().any(|x| k.eq_ignore_ascii_case(x)))
                .collect();
            world.connections = converted.connections;
            continue;
        }
        if !toggles.entities {
            if let Some(m) = brush_model(src) {
                model_solids.remove(&m);
            }
            continue;
        }
        let fallback = ids.object();
        let mut e = entities::convert(src, fallback, &mut used_ids, options.known_outputs.as_ref());
        if let Some(m) = brush_model(src) {
            if !toggles.brush_entities {
                model_solids.remove(&m);
            } else if let Some(solids) = model_solids.remove(&m) {
                report.entity_solids += solids.len();
                e.solids = solids;
            } else if !e.classname.eq_ignore_ascii_case("func_areaportal") {
                report.empty_brush_entities += 1;
            }
        }
        if e.classname.eq_ignore_ascii_case("func_areaportal")
            || e.classname.eq_ignore_ascii_case("func_areaportalwindow")
        {
            portal_entities.push(entities.len());
        }
        if e.classname.eq_ignore_ascii_case("func_occluder")
            && let Some(n) = e.get("occludernumber").and_then(|v| v.trim().parse::<usize>().ok())
        {
            occluder_entities.push((entities.len(), n));
        }
        entities.push(e);
    }

    let mut free_portals: Vec<Option<hp_vmf::Solid>> =
        if toggles.areaportals { portals.into_iter().map(Some).collect() } else { Vec::new() };
    for &ei in &portal_entities {
        let Some(key) = entities[ei].get("portalnumber").and_then(|v| v.trim().parse::<u16>().ok()) else { continue };
        let Some(center) = extras::portal_center(&l, key) else { continue };
        let found = free_portals.iter().position(|s| {
            s.as_ref().is_some_and(|s| {
                let g = hp_geom::build_solid(&s.sides);
                hp_geom::Aabb::new(g.aabb.min - DVec3::ONE, g.aabb.max + DVec3::ONE).contains_point(center)
            })
        });
        if let Some(i) = found
            && let Some(solid) = free_portals[i].take()
        {
            entities[ei].solids.push(solid);
            report.areaportals += 1;
        }
    }
    for solid in free_portals.into_iter().flatten() {
        let mut e = Entity { id: ids.object(), classname: "func_areaportal".to_owned(), ..Entity::default() };
        e.set("StartOpen", "1");
        e.solids.push(solid);
        report.areaportals += 1;
        entities.push(e);
    }

    report.visclusters = visclusters.len();
    for solid in visclusters {
        let mut e = Entity { id: ids.object(), classname: "func_viscluster".to_owned(), ..Entity::default() };
        e.solids.push(solid);
        entities.push(e);
    }

    for (ei, n) in occluder_entities {
        if !toggles.occluders {
            break;
        }
        if let Some(mut built) = extras::occluder(&l, n, map_name) {
            built.assign_ids(&mut ids);
            entities[ei].solids.push(built.solid);
            report.occluders += 1;
        }
    }
    report.failed_occluders = if toggles.occluders {
        entities.iter().filter(|e| e.solids.is_empty() && e.classname.eq_ignore_ascii_case("func_occluder")).count()
    } else {
        0
    };

    let bounds: Vec<(DVec3, DVec3)> = detail
        .iter()
        .map(|s| {
            let g = hp_geom::build_solid(&s.sides);
            (g.aabb.min, g.aabb.max)
        })
        .collect();
    let groups = if toggles.group_details {
        extras::touching_groups(&bounds)
    } else {
        for s in detail.drain(..) {
            world_solids.push(s);
        }
        Vec::new()
    };
    let mut detail: Vec<Option<hp_vmf::Solid>> = detail.into_iter().map(Some).collect();
    for group in groups {
        let mut e = Entity { id: ids.object(), classname: "func_detail".to_owned(), ..Entity::default() };
        e.solids = group.iter().filter_map(|&i| detail[i].take()).collect();
        report.detail_solids += e.solids.len();
        report.detail_entities += 1;
        entities.push(e);
    }

    let props = if toggles.static_props { extras::static_props(bsp, &mut report.warnings) } else { Vec::new() };
    for mut e in props {
        e.id = ids.object();
        report.static_props += 1;
        entities.push(e);
    }
    let (overlays, lost) =
        if toggles.overlays { extras::overlays(&l, &sides, &model_offsets, map_name) } else { (Vec::new(), 0) };
    report.overlays = overlays.len();
    report.overlays_without_sides = lost;
    for mut e in overlays {
        e.id = ids.object();
        entities.push(e);
    }
    let (water, lost) =
        if toggles.overlays { extras::water_overlays(&l, &sides, &model_offsets, map_name) } else { (Vec::new(), 0) };
    report.overlays += water.len();
    report.overlays_without_sides += lost;
    for mut e in water {
        e.id = ids.object();
        entities.push(e);
    }
    let cubemaps = if toggles.cubemaps { extras::cubemaps(bsp) } else { Vec::new() };
    for mut e in cubemaps {
        e.id = ids.object();
        report.cubemaps += 1;
        entities.push(e);
    }
    let mut leftover: Vec<(usize, Vec<hp_vmf::Solid>)> = model_solids.into_iter().collect();
    leftover.sort_by_key(|(m, _)| *m);
    for (_, solids) in leftover {
        world_solids.extend(solids);
    }

    report.world_solids = world_solids.len();
    world.solids = world_solids;
    let mut map = Map::new_empty();
    map.version_info.map_version = bsp.map_revision().max(1);
    map.world = world;
    map.entities = entities;
    Ok(Decompiled { map, report })
}

pub fn entities_only(text: &str, options: &Options) -> Decompiled {
    let source = hp_bsp::parse_entities(text);
    let mut report = Report { entities: source.len(), ..Report::default() };
    let max_hammer_id =
        source.iter().filter_map(|e| e.get("hammerid").and_then(|v| v.trim().parse::<Id>().ok())).max().unwrap_or(1);
    let mut next = max_hammer_id.saturating_add(1).max(2);
    let mut used_ids: HashSet<Id> = HashSet::from([1]);
    let mut world = Entity { id: 1, classname: "worldspawn".to_owned(), ..Entity::default() };
    let mut entities = Vec::with_capacity(source.len());
    for src in &source {
        if src.classname().eq_ignore_ascii_case("worldspawn") {
            let converted = entities::convert(src, 1, &mut HashSet::new(), options.known_outputs.as_ref());
            world.props = converted
                .props
                .into_iter()
                .filter(|(k, _)| !["world_mins", "world_maxs", "mapversion"].iter().any(|x| k.eq_ignore_ascii_case(x)))
                .collect();
            world.connections = converted.connections;
            continue;
        }
        if !options.toggles.entities {
            continue;
        }
        let e = entities::convert(src, next, &mut used_ids, options.known_outputs.as_ref());
        if e.id == next {
            next += 1;
        }
        if brush_model(src).is_some() {
            report.empty_brush_entities += 1;
        }
        entities.push(e);
    }
    report.warnings.push("only the entities were recovered: the map geometry was not available".to_owned());
    let mut map = Map::new_empty();
    map.world = world;
    map.entities = entities;
    Decompiled { map, report }
}

#[cfg(test)]
mod tests;
