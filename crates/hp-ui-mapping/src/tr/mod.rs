use hp_app::i18n::TrTable;

pub mod areaportals;
pub mod array;
pub mod autovis;
pub mod compile;
pub mod compile_stats;
pub mod compilers;
pub mod cordons;
pub mod cubemaps;
pub mod goto;
pub mod inspector;
pub mod instancing;
pub mod launch;
pub mod lighting;
pub mod mapassets;
pub mod mapinfo;
pub mod optimize;
pub mod outliner;
pub mod paste;
pub mod pointfile;
pub mod portals;
pub mod prefabs;
pub mod problems;
pub mod propper;
pub mod release;
pub mod scatter;
pub mod target;
pub mod transform;
pub mod valve_flags;
pub mod visgroups;

pub const TABLES: &[(&str, TrTable)] = &[
    ("visgroups", visgroups::TR),
    ("array", array::TR),
    ("autovis", autovis::TR),
    ("outliner", outliner::TR),
    ("compile", compile::TR),
    ("compilestats", compile_stats::TR),
    ("compilers", compilers::TR),
    ("problems", problems::TR),
    ("transform", transform::TR),
    ("paste", paste::TR),
    ("goto", goto::TR),
    ("mapinfo", mapinfo::TR),
    ("optimize", optimize::TR),
    ("cubemaps", cubemaps::TR),
    ("pointfile", pointfile::TR),
    ("cordons", cordons::TR),
    ("portals", portals::TR),
    ("areaportals", areaportals::TR),
    ("release", release::TR),
    ("scatter", scatter::TR),
    ("inspector", inspector::TR),
    ("lighting", lighting::TR),
    ("prefabs", prefabs::TR),
    ("propper", propper::TR),
    ("instancing", instancing::TR),
    ("launch", launch::TR),
    ("mapassets", mapassets::TR),
    ("target", target::TR),
    ("valveflags", valve_flags::TR),
];
