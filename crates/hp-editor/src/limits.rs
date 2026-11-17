pub mod areaportal;
pub mod breakable;
pub mod checks;
pub mod cost;
pub mod leak;
pub mod portals;
pub mod snapshot;
pub mod world;
pub mod world_light;

use std::collections::HashSet;

use checks::{CheckKnowledge, Materials};
use snapshot::MapSnapshot;

use crate::problems::{Problem, Severity};
use crate::selection::ObjRef;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Limit {
    Brushes,
    BrushSides,
    Planes,
    Entities,
    Models,
    Texinfo,
    Texdata,
    Overlays,
    Displacements,
    Cubemaps,
    WorldLights,
    Lightmaps,
    LightmapsHdr,
    Vertexes,
    Nodes,
    Faces,
    Edges,
    Surfedges,
}

impl Limit {
    pub const ALL: [Limit; 18] = [
        Limit::Brushes,
        Limit::BrushSides,
        Limit::Planes,
        Limit::Entities,
        Limit::Models,
        Limit::Texinfo,
        Limit::Texdata,
        Limit::Overlays,
        Limit::Displacements,
        Limit::Cubemaps,
        Limit::WorldLights,
        Limit::Lightmaps,
        Limit::LightmapsHdr,
        Limit::Vertexes,
        Limit::Nodes,
        Limit::Faces,
        Limit::Edges,
        Limit::Surfedges,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Limit::Brushes => "brushes",
            Limit::BrushSides => "brushsides",
            Limit::Planes => "planes",
            Limit::Entities => "entities",
            Limit::Models => "models",
            Limit::Texinfo => "texinfo",
            Limit::Texdata => "texdata",
            Limit::Overlays => "overlays",
            Limit::Displacements => "displacements",
            Limit::Cubemaps => "cubemaps",
            Limit::WorldLights => "worldlights",
            Limit::Lightmaps => "lightmaps",
            Limit::LightmapsHdr => "lightmaps_hdr",
            Limit::Vertexes => "vertexes",
            Limit::Nodes => "nodes",
            Limit::Faces => "faces",
            Limit::Edges => "edges",
            Limit::Surfedges => "surfedges",
        }
    }

    pub fn from_id(id: &str) -> Option<Limit> {
        Limit::ALL.into_iter().find(|l| l.id() == id)
    }

    pub fn table(self) -> hp_bsp::limits::MapLimit {
        use hp_bsp::limits as l;
        match self {
            Limit::Brushes => l::BRUSHES,
            Limit::BrushSides => l::BRUSHSIDES,
            Limit::Planes => l::PLANES,
            Limit::Entities => l::ENTITIES,
            Limit::Models => l::MODELS,
            Limit::Texinfo => l::TEXINFO,
            Limit::Texdata => l::TEXDATA,
            Limit::Overlays => l::OVERLAYS,
            Limit::Displacements => l::DISPINFO,
            Limit::Cubemaps => l::CUBEMAPSAMPLES,
            Limit::WorldLights => l::WORLDLIGHTS,
            Limit::Lightmaps | Limit::LightmapsHdr => l::LIGHTING,
            Limit::Vertexes => l::VERTEXES,
            Limit::Nodes => l::NODES,
            Limit::Faces => l::FACES,
            Limit::Edges => l::EDGES,
            Limit::Surfedges => l::SURFEDGES,
        }
    }

    pub fn is_bytes(self) -> bool {
        matches!(self, Limit::Lightmaps | Limit::LightmapsHdr)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LimitProfile {
    Stock,
    Gmod32,
    #[default]
    Gmod64,
    PlusPlus,
}

impl LimitProfile {
    pub const ALL: [LimitProfile; 4] =
        [LimitProfile::Stock, LimitProfile::Gmod32, LimitProfile::Gmod64, LimitProfile::PlusPlus];

    pub fn id(self) -> &'static str {
        match self {
            LimitProfile::Stock => "stock",
            LimitProfile::Gmod32 => "gmod32",
            LimitProfile::Gmod64 => "gmod64",
            LimitProfile::PlusPlus => "plusplus",
        }
    }

    pub fn max(self, limit: Limit) -> u64 {
        if self == LimitProfile::PlusPlus && limit == Limit::Entities {
            return 65_536;
        }
        let l = limit.table();
        if self == LimitProfile::Stock { l.sdk } else { l.gmod }
    }

    pub fn max_switchable_lights(self) -> usize {
        let l = hp_bsp::limits::SWITCHED_LIGHTS;
        (if self == LimitProfile::Stock { l.sdk } else { l.gmod }) as usize
    }

    pub fn check_options(self) -> checks::CheckOptions {
        checks::CheckOptions { max_switchable_lights: self.max_switchable_lights() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LimitUse {
    pub limit: Limit,
    pub used: u64,
    pub max: u64,
    pub top: Vec<(ObjRef, u64)>,
}

impl LimitUse {
    pub fn fraction(&self) -> f64 {
        if self.max == 0 { 0.0 } else { self.used as f64 / self.max as f64 }
    }
}

pub const MAX_AREAS: usize = hp_bsp::limits::AREAS.max_usize();

pub const LIMIT_WARNING: f64 = 0.8;

const TOP: usize = 8;

const NOT_IN_ENTITY_LUMP: &[&str] = &[
    "func_detail",
    "prop_static",
    "prop_detail",
    "info_overlay",
    "info_overlay_transition",
    "info_overlay_accessor",
    "env_cubemap",
    "func_instance",
    "func_instance_parms",
    "func_instance_origin",
    "func_instance_io_proxy",
    "info_null",
    "func_viscluster",
    "info_no_dynamic_shadow",
];

const NOT_MODELS: &[&str] =
    &["func_detail", "func_areaportal", "func_areaportalwindow", "func_viscluster", "func_occluder"];

fn class_in(class: &str, list: &[&str]) -> bool {
    list.iter().any(|c| class.eq_ignore_ascii_case(c))
}

fn top_of(mut v: Vec<(ObjRef, u64)>) -> Vec<(ObjRef, u64)> {
    v.retain(|(_, n)| *n > 0);
    v.sort_by_key(|b| std::cmp::Reverse(b.1));
    v.truncate(TOP);
    v
}

pub fn estimate_limits(snap: &MapSnapshot, k: &dyn CheckKnowledge, profile: LimitProfile) -> Vec<LimitUse> {
    let mut mats = Materials::new(snap, k);
    let solids: Vec<_> = snap.solids.iter().filter(|s| snap.is_visible(ObjRef::Solid(s.id))).collect();
    let entities: Vec<(usize, _)> =
        snap.entities.iter().enumerate().filter(|(_, r)| snap.is_visible(ObjRef::Entity(r.entity.id))).collect();
    let mut planes: HashSet<u64> = HashSet::new();
    let mut texinfo: HashSet<u64> = HashSet::new();
    let mut texdata: HashSet<u32> = HashSet::new();
    let (mut sides, mut disps, mut luxels) = (0u64, 0u64, 0u64);
    let (mut side_top, mut lux_top) = (Vec::new(), Vec::new());
    for s in &solids {
        let n = s.sides.len() as u64 + u64::from(s.bevels);
        sides += n;
        side_top.push((ObjRef::Solid(s.id), n));
        let mut lx = 0u64;
        for x in s.sides.iter() {
            if x.plane != 0 {
                planes.insert(x.plane);
            }
            texinfo.insert(x.texinfo);
            texdata.insert(x.material);
            if x.disp {
                disps += 1;
            }
            if mats.facts(x.material).lightmapped() {
                lx += u64::from(x.luxels);
            }
        }
        luxels += lx;
        lux_top.push((ObjRef::Solid(s.id), lx * 4));
    }
    let class_count =
        |list: &[&str]| entities.iter().filter(|(_, r)| class_in(&r.entity.classname, list)).count() as u64;
    let entity_lump = 1 + entities.iter().filter(|(_, r)| !class_in(&r.entity.classname, NOT_IN_ENTITY_LUMP)).count();
    let models = 1 + entities
        .iter()
        .filter(|(i, r)| {
            snap.entity_solids.get(*i).is_some_and(|n| *n > 0) && !class_in(&r.entity.classname, NOT_MODELS)
        })
        .count();
    let lights = class_count(&["light", "light_spot"]) + 2 * class_count(&["light_environment"]);
    let bevel_planes: u64 = solids.iter().map(|s| u64::from(s.bevels)).sum();
    let unique_planes = planes.len() as u64;
    let make =
        |limit: Limit, used: u64, top: Vec<(ObjRef, u64)>| LimitUse { limit, used, max: profile.max(limit), top };
    vec![
        make(Limit::Brushes, solids.len() as u64, Vec::new()),
        make(Limit::BrushSides, sides, top_of(side_top)),
        make(Limit::Planes, 2 * unique_planes + bevel_planes.min(2 * unique_planes), Vec::new()),
        make(Limit::Entities, entity_lump as u64, Vec::new()),
        make(Limit::Models, models as u64, Vec::new()),
        make(Limit::Texinfo, texinfo.len() as u64, Vec::new()),
        make(Limit::Texdata, texdata.len() as u64, Vec::new()),
        make(Limit::Overlays, class_count(&["info_overlay"]), Vec::new()),
        make(Limit::Displacements, disps, Vec::new()),
        make(Limit::Cubemaps, class_count(&["env_cubemap"]), Vec::new()),
        make(Limit::WorldLights, lights, Vec::new()),
        make(Limit::Lightmaps, luxels * 4, top_of(lux_top)),
    ]
}

pub fn exact_limits(counts: &[(&str, u64)], profile: LimitProfile) -> Vec<LimitUse> {
    let mut out: Vec<LimitUse> = counts
        .iter()
        .filter_map(|(name, used)| {
            let limit = Limit::from_id(name)?;
            Some(LimitUse { limit, used: *used, max: profile.max(limit), top: Vec::new() })
        })
        .collect();
    out.sort_by_key(|u| u.limit);
    out
}

pub fn worst(uses: &[LimitUse]) -> Option<&LimitUse> {
    uses.iter().filter(|u| u.max > 0).max_by(|a, b| a.fraction().total_cmp(&b.fraction()))
}

pub fn limit_problems(uses: &[LimitUse]) -> Vec<Problem> {
    let mut out = Vec::new();
    for u in uses {
        let f = u.fraction();
        if f < LIMIT_WARNING {
            continue;
        }
        let (severity, code) =
            if f > 1.0 { (Severity::Error, "limit_exceeded") } else { (Severity::Warning, "limit_close") };
        out.push(
            Problem::new(
                severity,
                code,
                format!("{}: {} of {} ({:.0}%)", u.limit.id(), u.used, u.max, f * 100.0),
                u.top.iter().map(|(o, _)| *o).collect(),
                false,
            )
            .with_args(vec![
                u.limit.id().to_owned(),
                u.used.to_string(),
                u.max.to_string(),
                format!("{:.0}", f * 100.0),
            ]),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use crate::scene::Owner;
    use crate::tests::box_sides;
    use checks::NoKnowledge;
    use glam::DVec3;

    #[test]
    fn estimates_count_what_vbsp_keeps() {
        let mut doc = Document::new_empty();
        for i in 0..4 {
            let o = DVec3::new(f64::from(i) * 128.0, 0.0, 0.0);
            doc.create_solid(box_sides(o, o + DVec3::splat(64.0), "BRICK/WALL"), Owner::World).expect("solid");
        }
        let nd = doc
            .create_solid(box_sides(DVec3::splat(512.0), DVec3::splat(576.0), "TOOLS/TOOLSNODRAW"), Owner::World)
            .expect("nodraw");
        let door = doc
            .create_solid(box_sides(DVec3::splat(704.0), DVec3::splat(768.0), "BRICK/WALL"), Owner::World)
            .expect("door");
        doc.tie_to_entity(&[ObjRef::Solid(door)], "func_door", &[]).expect("tie");
        let det = doc
            .create_solid(box_sides(DVec3::splat(896.0), DVec3::splat(960.0), "BRICK/WALL"), Owner::World)
            .expect("detail");
        doc.tie_to_entity(&[ObjRef::Solid(det)], "func_detail", &[]).expect("tie");
        doc.create_point_entity("prop_static", DVec3::ZERO, &[]).expect("prop");
        doc.create_point_entity("light", DVec3::ZERO, &[]).expect("light");
        doc.create_point_entity("light_environment", DVec3::ZERO, &[]).expect("sun");
        doc.create_point_entity("info_overlay", DVec3::ZERO, &[]).expect("overlay");
        doc.create_point_entity("env_cubemap", DVec3::ZERO, &[]).expect("cubemap");
        let snap = MapSnapshot::capture(&doc);
        let uses = estimate_limits(&snap, &NoKnowledge, LimitProfile::Stock);
        let get = |l: Limit| uses.iter().find(|u| u.limit == l).map(|u| u.used).expect("limit");
        assert_eq!(get(Limit::Brushes), 7);
        assert_eq!(get(Limit::BrushSides), 42);
        assert_eq!(get(Limit::Entities), 4);
        assert_eq!(get(Limit::Models), 2);
        assert_eq!(get(Limit::Texdata), 2);
        assert_eq!(get(Limit::Overlays), 1);
        assert_eq!(get(Limit::Cubemaps), 1);
        assert_eq!(get(Limit::WorldLights), 3);
        assert_eq!(get(Limit::Lightmaps), 6 * 6 * 25 * 4);
        let lux = uses.iter().find(|u| u.limit == Limit::Lightmaps).expect("lightmaps");
        assert!(!lux.top.iter().any(|(o, _)| *o == ObjRef::Solid(nd)));
        assert!(limit_problems(&uses).is_empty());
        doc.set_hidden(&[ObjRef::Solid(nd)], true);
        let snap = MapSnapshot::capture(&doc);
        let uses = estimate_limits(&snap, &NoKnowledge, LimitProfile::Stock);
        assert_eq!(uses.iter().find(|u| u.limit == Limit::Brushes).map(|u| u.used), Some(6));
    }

    #[test]
    fn profiles_exact_counts_and_problems() {
        assert_eq!(LimitProfile::Stock.max(Limit::Overlays), 512);
        assert_eq!(LimitProfile::Gmod64.max(Limit::Overlays), 8192);
        assert_eq!(LimitProfile::PlusPlus.max(Limit::Overlays), 8192);
        assert_eq!(LimitProfile::Gmod64.max(Limit::Brushes), 65_535);
        assert_eq!(LimitProfile::Gmod32.max(Limit::Brushes), 65_535);
        assert_eq!(LimitProfile::Gmod64.max(Limit::BrushSides), 655_350);
        assert_eq!(LimitProfile::Gmod64.max(Limit::Displacements), 16_384);
        assert_eq!((LimitProfile::Gmod64.max(Limit::Models), LimitProfile::Stock.max(Limit::Models)), (4_096, 1_024));
        assert_eq!(LimitProfile::Gmod64.max(Limit::Entities), 65_535);
        assert_eq!(LimitProfile::PlusPlus.max(Limit::Entities), 65_536);
        assert_eq!(LimitProfile::Stock.max(Limit::Brushes), 8_192);
        assert_eq!(LimitProfile::PlusPlus.max_switchable_lights(), 96);
        assert_eq!(LimitProfile::Gmod64.max_switchable_lights(), 96);
        assert_eq!(LimitProfile::Stock.max_switchable_lights(), 32);
        let uses = exact_limits(&[("brushes", 9000), ("planes", 100), ("unknown", 5)], LimitProfile::Stock);
        assert_eq!(uses.len(), 2);
        assert_eq!(worst(&uses).map(|u| u.limit), Some(Limit::Brushes));
        let p = limit_problems(&uses);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].code, "limit_exceeded");
        assert_eq!(p[0].message, "brushes: 9000 of 8192 (110%)");
        let close = limit_problems(&[LimitUse { limit: Limit::Planes, used: 60_000, max: 65_536, top: Vec::new() }]);
        assert_eq!(close[0].code, "limit_close");
        for l in Limit::ALL {
            assert_eq!(Limit::from_id(l.id()), Some(l));
        }
    }
}
