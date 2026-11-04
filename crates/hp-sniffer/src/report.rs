use std::path::PathBuf;

use crate::assets::AssetReport;
use crate::recover::MapCounts;
use crate::scan::{Coverage, LumpCoverage};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VmfCounts {
    pub solids: usize,
    pub world_solids: usize,
    pub entity_solids: usize,
    pub sides: usize,
    pub displacement_sides: usize,
    pub entities: usize,
    pub connections: usize,
    pub nodraw_sides: usize,
    pub tool_sides: usize,
    pub sky_sides: usize,
    pub clip_sides: usize,
    pub trigger_sides: usize,
    pub props: usize,
    pub overlays: usize,
    pub cubemaps: usize,
    pub details: usize,
    pub materials: usize,
}

pub fn vmf_counts(map: &hp_vmf::Map) -> VmfCounts {
    let mut c =
        VmfCounts { entities: map.entities.len(), world_solids: map.world.solids.len(), ..VmfCounts::default() };
    let mut materials = std::collections::HashSet::new();
    let mut side_stats = |solids: &[hp_vmf::Solid], c: &mut VmfCounts| {
        for s in solids {
            c.solids += 1;
            for side in &s.sides {
                c.sides += 1;
                if side.disp.is_some() {
                    c.displacement_sides += 1;
                }
                let name = side.material.to_ascii_lowercase();
                materials.insert(name.clone());
                if name.ends_with("toolsnodraw") {
                    c.nodraw_sides += 1;
                } else if name.ends_with("toolsskybox") || name.starts_with("skybox/") {
                    c.sky_sides += 1;
                } else if name.contains("toolsclip")
                    || name.contains("toolsplayerclip")
                    || name.contains("toolsnpcclip")
                {
                    c.clip_sides += 1;
                } else if name.contains("toolstrigger") {
                    c.trigger_sides += 1;
                } else if name.starts_with("tools/") {
                    c.tool_sides += 1;
                }
            }
        }
    };
    side_stats(&map.world.solids, &mut c);
    for e in &map.entities {
        c.connections += e.connections.len();
        let before = c.solids;
        side_stats(&e.solids, &mut c);
        c.entity_solids += c.solids - before;
        let class = e.classname.to_ascii_lowercase();
        if class.starts_with("prop_") {
            c.props += 1;
        } else if class == "info_overlay" || class == "info_overlay_transition" {
            c.overlays += 1;
        } else if class == "env_cubemap" {
            c.cubemaps += 1;
        } else if class == "func_detail" {
            c.details += 1;
        }
    }
    c.materials = materials.len();
    c
}

#[derive(Clone, Debug, PartialEq)]
pub struct CoverageRow {
    pub lump: usize,
    pub name: String,
    pub len: usize,
    pub samples_hit: usize,
    pub samples_total: usize,
    pub match_percent: f64,
    pub state: String,
    pub address: Option<u64>,
}

pub fn coverage_rows(cov: &[LumpCoverage]) -> Vec<CoverageRow> {
    cov.iter()
        .map(|c| CoverageRow {
            lump: c.lump,
            name: hp_bsp::lump_name(c.lump).to_owned(),
            len: c.len,
            samples_hit: c.samples_hit,
            samples_total: c.samples_total,
            match_percent: c.match_percent,
            state: match c.state {
                Coverage::Verified => "verified",
                Coverage::Mostly => "mostly",
                Coverage::Partial => "partial",
                Coverage::Absent => "runtime format or absent",
            }
            .to_owned(),
            address: c.base,
        })
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CmReport {
    pub found: bool,
    pub brush_address: u64,
    pub brushes: usize,
    pub axial: usize,
    pub planes: usize,
    pub sides: usize,
    pub surfaces: usize,
    pub nodraw: usize,
    pub tools: usize,
    pub sky: usize,
    pub clip: usize,
    pub trigger: usize,
    pub other: usize,
    pub compared: crate::cm::CmComparison,
    pub identical: bool,
}

pub fn cm_report(cm: &crate::cm::CmFindings, cmp: crate::cm::CmComparison) -> CmReport {
    let k = cm.side_kinds();
    CmReport {
        found: true,
        brush_address: cm.brush_addr,
        brushes: cm.brushes.len(),
        axial: cm.axial_count(),
        planes: cm.planes,
        sides: cm.total_sides(),
        surfaces: cm.surface_names.len(),
        nodraw: k.nodraw,
        tools: k.tools,
        sky: k.sky,
        clip: k.clip,
        trigger: k.trigger,
        other: k.other,
        identical: cmp.identical(),
        compared: cmp,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemoryRecovery {
    pub found: bool,
    pub render_found: bool,
    pub faces: usize,
    pub texinfo: usize,
    pub vertexes: usize,
    pub displacements: usize,
    pub overlays: usize,
    pub props: usize,
    pub prop_models: usize,
    pub cubemaps: usize,
    pub area_portals: usize,
    pub area_portal_verts: usize,
    pub occluders: usize,
    pub faces_address: u64,
    pub texinfo_address: u64,
}

#[derive(Clone, Debug, Default)]
pub struct SniffReport {
    pub passthrough: crate::passthrough::Passthrough,
    pub hub: crate::hubdata::HubReport,
    pub unprotect: crate::unprotect::UnprotectReport,
    pub client: crate::clientents::ClientRecovery,
    pub map: String,
    pub server_name: String,
    pub address: String,
    pub game_pid: Option<u32>,
    pub game_is_64bit: Option<bool>,
    pub vac_secure: Option<bool>,
    pub insecure_flag: bool,
    pub launched_by_us: bool,
    pub source: String,
    pub geometry: String,
    pub memory_only: bool,
    pub memory_entities: usize,
    pub memory_entity_bytes: usize,
    pub memory_entity_address: Option<u64>,
    pub file_entities: usize,
    pub entities_identical: bool,
    pub entities_replaced: bool,
    pub coverage: Vec<CoverageRow>,
    pub verified_lumps: usize,
    pub verified_bytes: u64,
    pub bsp_counts: MapCounts,
    pub cm: CmReport,
    pub memory: MemoryRecovery,
    pub map_file_mode: String,
    pub map_file_opens: usize,
    pub cross_check: Vec<crate::recover::CrossRow>,
    pub merge: crate::merge::MergeStats,
    pub sources: Vec<crate::merge::SourceRow>,
    pub vmf: VmfCounts,
    pub decompile: hp_decompile::Report,
    pub assets: AssetReport,
    pub timings: Vec<(String, f64)>,
    pub notes: Vec<String>,
    pub vmf_path: PathBuf,
    pub bsp_path: Option<PathBuf>,
    pub content_dir: Option<PathBuf>,
    pub report_path: Option<PathBuf>,
}

impl SniffReport {
    pub fn render_text(&self) -> String {
        let mut o = String::new();
        let mut line = |s: String| {
            o.push_str(&s);
            o.push('\n');
        };
        line("BSP Sniffer report (Garry's Mod only)".to_owned());
        line(format!("server: {} ({})", self.server_name, self.address));
        line(format!(
            "map: {}   game process: {}   64 bit: {}   VAC secure server: {}   -insecure: {}   launched by Hammer Pro: {}",
            self.map,
            self.game_pid.map(|p| p.to_string()).unwrap_or_else(|| "-".to_owned()),
            self.game_is_64bit.map(|b| b.to_string()).unwrap_or_else(|| "?".to_owned()),
            self.vac_secure.map(|b| b.to_string()).unwrap_or_else(|| "?".to_owned()),
            self.insecure_flag,
            self.launched_by_us
        ));
        if self.unprotect.protected() {
            let u = &self.unprotect;
            line(format!(
                "protected map (Hammer Pro protector): {} scrambled material names, {} guessed from the loaded materials ({} ambiguous), {} unresolved (renamed custom materials cannot be named)",
                u.garbage_names, u.guessed, u.guess_ambiguous, u.unresolved
            ));
            if let Some(w) = &u.watermark {
                line(format!("watermark found in the map: {w}"));
            }
            if let Some(h) = &u.lock_hash {
                line(format!("server lock hash found in the map: {h}"));
            }
        }
        if self.client.attempted {
            let c = &self.client;
            match &c.error {
                Some(e) => line(format!("client entities: not read ({e})")),
                None => {
                    line(format!(
                        "client entities (networked entities of the client): {} read in {} reads, {} created by the map, {} without a class name, {} matched to the entity lump, {} added, {} parent names restored",
                        c.networked, c.observations, c.map_created, c.unnamed, c.matched, c.added, c.parents_restored
                    ));
                    if !c.added_classes.is_empty() {
                        let list: Vec<String> = c.added_classes.iter().map(|(n, k)| format!("{n} {k}")).collect();
                        line(format!("entities added from the client: {}", list.join(", ")));
                    }
                    if c.original_at_least > 0 {
                        line(format!(
                            "the map creation ids show the original entity lump had at least {} entities: the file and the client give {}, at least {} (logic, nodes, paths and the other entities the server never sends) cannot be recovered",
                            c.original_at_least,
                            c.lump_entities + c.added,
                            c.unrecoverable_at_least
                        ));
                    }
                    for n in &c.notes {
                        line(format!("client entities note: {n}"));
                    }
                }
            }
        }
        if !self.passthrough.names().is_empty() {
            line(format!(
                "lumps taken from the map file as they are ({} bytes), because the structures they index are identical: {}",
                self.passthrough.bytes(),
                self.passthrough.names().join(", ")
            ));
        }
        for l in self.hub.render() {
            line(l);
        }
        line(format!("geometry source: {}", self.source));
        line(format!("geometry rebuilt from: {}", self.geometry));
        line(format!(
            "entity lump in game memory: {} entities, {} bytes, at 0x{:x}",
            self.memory_entities,
            self.memory_entity_bytes,
            self.memory_entity_address.unwrap_or(0)
        ));
        if !self.memory_only {
            line(format!(
                "entity lump of the file: {} entities, identical to memory: {}, replaced by the memory copy: {}",
                self.file_entities, self.entities_identical, self.entities_replaced
            ));
            line(format!(
                "lumps found byte identical in game memory: {} ({} bytes)",
                self.verified_lumps, self.verified_bytes
            ));
            for r in &self.coverage {
                line(format!(
                    "  lump {:2} {:<28} {:>10} bytes  samples {:>2}/{:<2}  match {:6.2}%  {}{}",
                    r.lump,
                    r.name,
                    r.len,
                    r.samples_hit,
                    r.samples_total,
                    r.match_percent,
                    r.state,
                    r.address.map(|a| format!("  at 0x{a:x}")).unwrap_or_default()
                ));
            }
        }
        let m = &self.cm;
        if m.found {
            line(format!(
                "collision data decoded from the game memory (no file involved), brush table at 0x{:x}:",
                m.brush_address
            ));
            line(format!(
                "  brushes {} (axis aligned boxes {}), planes {}, brush sides {}, materials {}",
                m.brushes, m.axial, m.planes, m.sides, m.surfaces
            ));
            line(format!(
                "  sides by material: nodraw {}, other tools {}, skybox {}, clip {}, trigger {}, other {}",
                m.nodraw, m.tools, m.sky, m.clip, m.trigger, m.other
            ));
            let c = &m.compared;
            if c.brushes_compared > 0 || !self.memory_only {
                line(format!(
                    "  compared with the map file: brushes {} compared, {} different; sides {} compared, {} with another plane; boxes {} compared, {} different; brushes whose materials differ {}; identical: {}",
                    c.brushes_compared,
                    c.brush_mismatch,
                    c.sides_compared,
                    c.side_plane_mismatch,
                    c.boxes_compared,
                    c.box_mismatch,
                    c.material_mismatch_brushes,
                    m.identical
                ));
            }
        } else if !self.memory_only {
            line("collision data: not found in the game memory".to_owned());
        }
        let mr = &self.memory;
        if mr.found {
            line(format!(
                "render data decoded from the game memory: faces {} (at 0x{:x}), texinfo {} (at 0x{:x}), vertexes {}, displacements {}, static props {} ({} models), overlays {}, cubemaps {}, area portals {} ({} vertexes)",
                mr.faces,
                mr.faces_address,
                mr.texinfo,
                mr.texinfo_address,
                mr.vertexes,
                mr.displacements,
                mr.props,
                mr.prop_models,
                mr.overlays,
                mr.cubemaps,
                mr.area_portals,
                mr.area_portal_verts
            ));
        }
        line(format!(
            "map file setting: {}   map file reads by Hammer Pro: {}",
            if self.map_file_mode.is_empty() { "compare" } else { &self.map_file_mode },
            self.map_file_opens
        ));
        if !self.cross_check.is_empty() {
            line("cross check, game memory versus the map file (optional)".to_owned());
            for r in &self.cross_check {
                line(format!(
                    "  {:<18} memory {:>7}  file {:>7}  {}",
                    r.name,
                    r.memory,
                    r.file,
                    if r.memory == r.file { "equal" } else { "different" }
                ));
            }
        }
        if !self.sources.is_empty() {
            line("source of each category".to_owned());
            for r in &self.sources {
                line(format!("  {:<50} {:<22} {}", r.category, r.source, r.detail));
            }
        }
        let b = &self.bsp_counts;
        let v = &self.vmf;
        line("counts, game memory versus recovered BSP versus produced VMF".to_owned());
        if m.found {
            line(format!(
                "  brushes: memory {}  bsp {}  vmf solids {} (world {}, entities {}, of which displacements {})",
                m.brushes, b.brushes, v.solids, v.world_solids, v.entity_solids, b.displacements
            ));
            line(format!("  brush sides: memory {}  bsp {}  vmf sides {}", m.sides, b.brush_sides, v.sides));
            line(format!("  nodraw sides: memory {}  bsp {}  vmf {}", m.nodraw, b.nodraw_sides, v.nodraw_sides));
            line(format!("  planes: memory {}  bsp {}", m.planes, b.planes));
        }
        let d0 = &self.decompile;
        line(format!(
            "  entities: memory {}  bsp {}  vmf {} (source entities {} + {} static props, {} overlays, {} cubemaps, {} func_detail groups)",
            self.memory_entities,
            b.entities,
            v.entities,
            d0.entities,
            d0.static_props,
            d0.overlays,
            d0.cubemaps,
            d0.detail_entities
        ));
        line(format!(
            "  brushes: bsp {}  vmf solids {} (world {}, entities {})",
            b.brushes, v.solids, v.world_solids, v.entity_solids
        ));
        line(format!("  brush sides: bsp {}  vmf sides {}", b.brush_sides, v.sides));
        line(format!("  displacements: bsp {}  vmf {}", b.displacements, v.displacement_sides));
        line(format!(
            "  sides by kind in the vmf: nodraw {}, other tools {}, skybox {}, clip {}, trigger {}",
            v.nodraw_sides, v.tool_sides, v.sky_sides, v.clip_sides, v.trigger_sides
        ));
        line(format!(
            "  sides by kind in the bsp: nodraw {}, other tools {}, skybox {}, clip {}, trigger {}",
            b.nodraw_sides, b.tool_sides, b.sky_sides, b.clip_sides, b.trigger_sides
        ));
        line(format!("  planes {}  vertexes {}  faces {}  materials {}", b.planes, b.vertexes, b.faces, b.materials));
        line(format!(
            "  props {}  overlays {}  cubemaps {}  func_detail {}  outputs {}",
            v.props, v.overlays, v.cubemaps, v.details, v.connections
        ));
        let d = &self.decompile;
        line(format!(
            "decompiler: world {}, detail {} in {} groups, entity solids {}, failed {}, empty {}, areaportals {}, occluders {}",
            d.world_solids,
            d.detail_solids,
            d.detail_entities,
            d.entity_solids,
            d.failed_brushes,
            d.empty_brushes,
            d.areaportals,
            d.occluders
        ));
        if d.orphan_models > 0 {
            line(format!(
                "brush models no entity uses (their entity was removed): {} made into brush entities by the decompiler",
                d.orphan_models
            ));
        }
        let a = &self.assets;
        line(format!(
            "assets: pakfile {} files ({} bytes, {} stock skipped), game content {} files ({} bytes, {} stock skipped, {} failed, {} missing)",
            a.pak_files,
            a.pak_bytes,
            a.pak_skipped_stock,
            a.content_files,
            a.content_bytes,
            a.stock_skipped,
            a.content_failed,
            a.missing.len()
        ));
        if a.memory_models_wanted > 0 || a.memory_models > 0 {
            line(format!(
                "  models recovered from the game memory: {} of {} wanted, {} bytes of .mdl; vertex files (.vvd) rebuilt {} ({} bytes, {} with vertices the engine dropped, {} not found)",
                a.memory_models,
                a.memory_models_wanted,
                a.memory_model_bytes,
                a.memory_vvds,
                a.memory_vvd_bytes,
                a.memory_vvd_partial,
                a.memory_vvd_failed
            ));
            line(format!(
                "  strip files (.dx90.vtx) rebuilt {} ({} bytes, {} not whole, {} failed); physics files (.phy) rebuilt {} ({} bytes)",
                a.memory_vtxs,
                a.memory_vtx_bytes,
                a.memory_vtx_partial,
                a.memory_vtx_failed,
                a.memory_phys,
                a.memory_phy_bytes
            ));
        }
        if a.textures_loaded > 0 {
            line(format!(
                "  textures: {} loaded by the engine, {} not on this computer and kept by the video card only ({} bytes)",
                a.textures_loaded, a.textures_video_only, a.textures_video_only_bytes
            ));
            if a.textures_with_file > 0 {
                line(format!(
                    "  texture copies in the process memory: of {} textures with a file on this computer {} have the whole file resident, {} partly, the others live in video memory only; {} whole texture files ({} bytes) are resident in all",
                    a.textures_with_file,
                    a.textures_resident_whole,
                    a.textures_resident_partial,
                    a.vtf_blobs_resident,
                    a.vtf_blobs_resident_bytes
                ));
            }
        }
        if a.pak_listed > 0 {
            line(format!(
                "  pakfile directory held by the game: {} files, {} identical to the recovered file (size and checksum), {} recovered but different, the others not recovered",
                a.pak_listed, a.pak_listed_identical, a.pak_listed_differs
            ));
        }
        if a.memory_materials_loaded > 0 {
            line(format!(
                "  materials rebuilt from the key values in the game memory: {} not on this computer, of {} loaded ({} bytes)",
                a.memory_materials, a.memory_materials_loaded, a.memory_material_bytes
            ));
        }
        for (k, n) in &a.by_class {
            line(format!("  {k}: {n}"));
        }
        for m in a.missing.iter().take(40) {
            line(format!("  missing: {m}"));
        }
        if a.missing.len() > 40 {
            line(format!("  ... and {} more missing", a.missing.len() - 40));
        }
        for (k, secs) in &self.timings {
            line(format!("time {k}: {secs:.1} s"));
        }
        for n in self.notes.iter().chain(self.decompile.warnings.iter()) {
            line(format!("note: {n}"));
        }
        line(format!("vmf: {}", self.vmf_path.display()));
        if let Some(p) = &self.bsp_path {
            line(format!("bsp: {}", p.display()));
        }
        if let Some(p) = &self.content_dir {
            line(format!("content: {}", p.display()));
        }
        o
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_classify_tool_textures_everywhere() {
        let vmf = r#"
world
{
	"id" "1"
	"classname" "worldspawn"
	solid
	{
		"id" "2"
		side { "id" "1" "plane" "(0 0 0) (0 1 0) (1 1 0)" "material" "TOOLS/TOOLSNODRAW" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "2" "plane" "(0 0 8) (1 1 8) (0 1 8)" "material" "tools/toolsskybox" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "3" "plane" "(0 0 8) (1 1 8) (0 1 9)" "material" "tools/toolsplayerclip" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "4" "plane" "(0 0 8) (1 1 8) (0 1 10)" "material" "tools/toolstrigger" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "5" "plane" "(0 0 8) (1 1 8) (0 1 11)" "material" "tools/toolsblocklight" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
		side { "id" "6" "plane" "(0 0 8) (1 1 8) (0 1 12)" "material" "brick/wall" "uaxis" "[1 0 0 0] 0.25" "vaxis" "[0 -1 0 0] 0.25" "rotation" "0" "lightmapscale" "16" "smoothing_groups" "0" }
	}
}
entity
{
	"id" "10"
	"classname" "prop_static"
	"model" "models/a.mdl"
	connections
	{
		"OnX" "a,Kill,,0,-1"
	}
}
entity
{
	"id" "11"
	"classname" "env_cubemap"
}
"#;
        let map = hp_vmf::parse(vmf).expect("vmf");
        let c = vmf_counts(&map);
        assert_eq!((c.solids, c.sides), (1, 6));
        assert_eq!((c.nodraw_sides, c.sky_sides, c.clip_sides, c.trigger_sides, c.tool_sides), (1, 1, 1, 1, 1));
        assert_eq!((c.props, c.cubemaps, c.connections, c.entities), (1, 1, 1, 2));
        assert_eq!(c.materials, 6);
    }

    #[test]
    fn the_text_report_has_the_proof_lines() {
        let mut r = SniffReport { map: "gm_x".into(), vmf_path: PathBuf::from("a.vmf"), ..SniffReport::default() };
        r.memory_entities = 12;
        r.file_entities = 12;
        r.bsp_counts.entities = 12;
        r.bsp_counts.brushes = 5;
        r.vmf.solids = 6;
        r.cm = CmReport { found: true, brushes: 5, sides: 30, nodraw: 4, planes: 40, ..CmReport::default() };
        let text = r.render_text();
        assert!(text.contains("entities: memory 12  bsp 12"), "{text}");
        assert!(text.contains("brushes: memory 5  bsp 5  vmf solids 6"), "{text}");
        assert!(text.contains("nodraw sides: memory 4"), "{text}");
        assert!(!text.contains('\u{2014}'));
        r.cm = CmReport::default();
        assert!(r.render_text().contains("collision data: not found"));
        r.memory_only = true;
        assert!(!r.render_text().contains("collision data: not found"));
    }
}
