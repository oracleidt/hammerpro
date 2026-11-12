use glam::DVec3;
use hp_geom::Aabb;

use super::{Finding, Impact, Topic, fmt_point};
use crate::limits::cost::{CostLumps, RenderCosts};
use crate::limits::portals::{BspVis, Portal, PortalEntity, PortalFile, lump};

const AREAS_LUMP: usize = 20;
const AREA_PORTALS_LUMP: usize = 21;
const CLIP_PORTAL_VERTS_LUMP: usize = 41;
const SMALL_DIM: f64 = 24.0;
const SMALL_VOLUME: f64 = 48.0 * 48.0 * 48.0;
const MAX_WORST: usize = 12;
const MAX_SMALL_BOXES: usize = 64;
const SAMPLE_LIMIT: usize = 12_000;
const OPEN_MAP_MIN: u32 = 40;
const WORST_MIN: u32 = 24;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AreaStat {
    pub area: u16,
    pub clusters: usize,
    pub leaves: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClusterStat {
    pub cluster: u32,
    pub area: u16,
    pub bounds: Aabb,
    pub visible: u32,
    pub portals: u32,
    pub triangles: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AreaportalState {
    pub number: u16,
    pub entity: u32,
    pub name: String,
    pub door: String,
    pub window: bool,
    pub areas: Vec<(u16, u16)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisReport {
    pub leaves: usize,
    pub empty_leaves: usize,
    pub clusters: u32,
    pub portals: usize,
    pub areas: usize,
    pub area_stats: Vec<AreaStat>,
    pub has_vis: bool,
    pub total_pvs: u64,
    pub average_pvs: f64,
    pub max_pvs: u32,
    pub vis_work: u64,
    pub average_triangles: f64,
    pub worst: Vec<ClusterStat>,
    pub small_clusters: usize,
    pub small_boxes: Vec<Aabb>,
    pub areaportals: Vec<AreaportalState>,
    pub log_warnings: Vec<String>,
}

pub struct BspAnalysis {
    pub report: VisReport,
    pub vis: BspVis,
    pub portals: Option<PortalFile>,
    pub costs: Option<RenderCosts>,
}

fn i32_at(d: &[u8], o: usize) -> Option<i32> {
    d.get(o..o + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn i16_at(d: &[u8], o: usize) -> Option<i16> {
    d.get(o..o + 2).map(|b| i16::from_le_bytes([b[0], b[1]]))
}

pub fn portal_entities(entities: &[hp_bsp::entities::Entity]) -> Vec<(PortalEntity, u32)> {
    entities
        .iter()
        .filter_map(|e| {
            let id = e.get("hammerid").and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(0);
            let class = e.get("classname")?.to_ascii_lowercase();
            let window = class == "func_areaportalwindow";
            if class != "func_areaportal" && !window {
                return None;
            }
            Some((
                PortalEntity {
                    number: e.get("portalnumber")?.trim().parse().ok()?,
                    door: e.get("target").unwrap_or_default().trim().to_owned(),
                    name: e.get("targetname").unwrap_or_default().trim().to_owned(),
                    start_open: e.get("StartOpen").is_none_or(|v| v.trim() != "0"),
                    window,
                    fade_dist: 0,
                },
                id,
            ))
        })
        .collect()
}

pub fn analyze_bsp(bsp: &hp_bsp::Bsp, portals: Option<PortalFile>, log: &[String]) -> Result<BspAnalysis, String> {
    let get = |i: usize| bsp.lump(i).map(|c| c.into_owned()).unwrap_or_default();
    let leaf_version = bsp.lump_info(lump::LEAFS).map_err(|e| e.to_string())?.version;
    let (planes, nodes, leafs, models, visdata) =
        (get(lump::PLANES), get(lump::NODES), get(lump::LEAFS), get(lump::MODELS), get(lump::VISIBILITY));
    let mut vis =
        BspVis::from_lumps(&planes, &nodes, &leafs, leaf_version, &models, &visdata).map_err(|e| e.to_string())?;
    let entities = bsp.entities().unwrap_or_default();
    let portal_list = portal_entities(&entities);
    vis.set_area_portals(
        &get(AREAS_LUMP),
        &get(AREA_PORTALS_LUMP),
        &get(CLIP_PORTAL_VERTS_LUMP),
        portal_list.iter().map(|(p, _)| p.clone()).collect(),
    );
    let costs = vis.has_visibility().then(|| {
        let props = bsp
            .game_lumps()
            .ok()
            .and_then(|g| g.get(hp_bsp::gamelump::id::STATIC_PROPS).and_then(|p| p.data().ok().map(|d| d.into_owned())))
            .unwrap_or_default();
        let (leaf_faces, faces, texinfo, dispinfo, overlays) =
            (get(lump::LEAF_FACES), get(lump::FACES), get(lump::TEXINFO), get(lump::DISPINFO), get(lump::OVERLAYS));
        RenderCosts::from_lumps(
            &CostLumps {
                leafs: &leafs,
                leaf_version,
                leaf_faces: &leaf_faces,
                faces: &faces,
                texinfo: &texinfo,
                dispinfo: &dispinfo,
                overlays: &overlays,
                static_props: &props,
            },
            &vis,
        )
    });
    let leaf_size = if leaf_version == 0 { 56 } else { 32 };
    let empty_leaves = leafs
        .chunks_exact(leaf_size)
        .filter(|c| i32_at(c, 0).unwrap_or(1) & 1 == 0 && i16_at(c, 4).unwrap_or(-1) >= 0)
        .count();
    let report = build_report(&vis, portals.as_ref(), costs.as_ref(), empty_leaves, &portal_list, log);
    Ok(BspAnalysis { report, vis, portals, costs })
}

pub fn build_report(
    vis: &BspVis,
    portals: Option<&PortalFile>,
    costs: Option<&RenderCosts>,
    empty_leaves: usize,
    portal_list: &[(PortalEntity, u32)],
    log: &[String],
) -> VisReport {
    let bounds = vis.cluster_bounds();
    let cluster_count = bounds.len().max(vis.cluster_count() as usize);
    let cluster_areas = vis.cluster_areas();
    let mut per_cluster_portals = vec![0u32; cluster_count];
    if let Some(p) = portals {
        for portal in &p.portals {
            for c in portal.clusters {
                if let Some(n) = per_cluster_portals.get_mut(c as usize) {
                    *n += 1;
                }
            }
        }
    }
    let mut area_map: std::collections::BTreeMap<u16, AreaStat> = std::collections::BTreeMap::new();
    for (cluster, area, _) in vis.leaves() {
        if cluster < 0 {
            continue;
        }
        let s = area_map.entry(area).or_insert(AreaStat { area, ..AreaStat::default() });
        s.leaves += 1;
    }
    for a in &cluster_areas {
        area_map.entry(*a).or_insert(AreaStat { area: *a, ..AreaStat::default() }).clusters += 1;
    }
    let has_vis = vis.has_visibility();
    let mut visible = vec![0u32; cluster_count];
    let mut total_pvs = 0u64;
    let mut vis_work = 0u64;
    let step = (cluster_count / SAMPLE_LIMIT).max(1);
    if has_vis {
        for c in 0..cluster_count {
            let Some(row) = vis.visible_from(c as u32) else { continue };
            let seen = row.iter().filter(|v| **v).count() as u32;
            visible[c] = seen;
            total_pvs += u64::from(seen);
            if c % step == 0 {
                let reach: u64 = row
                    .iter()
                    .enumerate()
                    .filter(|(_, v)| **v)
                    .map(|(i, _)| u64::from(per_cluster_portals.get(i).copied().unwrap_or(0)))
                    .sum();
                vis_work = vis_work.saturating_add(
                    u64::from(per_cluster_portals[c]).saturating_mul(reach).saturating_mul(step as u64),
                );
            }
        }
    }
    let heat = costs.map(RenderCosts::heat).unwrap_or(&[]);
    let mut order: Vec<usize> = (0..cluster_count).collect();
    order.sort_by(|a, b| {
        visible[*b].cmp(&visible[*a]).then_with(|| {
            let va = bounds.get(*a).map_or(0.0, Aabb::volume);
            let vb = bounds.get(*b).map_or(0.0, Aabb::volume);
            vb.total_cmp(&va)
        })
    });
    let worst = order
        .iter()
        .take(MAX_WORST)
        .filter(|c| has_vis && visible[**c] > 0)
        .map(|&c| ClusterStat {
            cluster: c as u32,
            area: cluster_areas.get(c).copied().unwrap_or(0),
            bounds: bounds.get(c).copied().unwrap_or(Aabb::EMPTY),
            visible: visible[c],
            portals: per_cluster_portals[c],
            triangles: heat.get(c).copied().unwrap_or(0),
        })
        .collect();
    let small: Vec<Aabb> = bounds
        .iter()
        .filter(|b| !b.is_empty() && (b.size().min_element() < SMALL_DIM || b.volume() < SMALL_VOLUME))
        .copied()
        .collect();
    let pairs = vis.area_portal_pairs();
    let areaportals = portal_list
        .iter()
        .map(|(p, id)| AreaportalState {
            number: p.number,
            entity: *id,
            name: p.name.clone(),
            door: p.door.clone(),
            window: p.window,
            areas: pairs.iter().filter(|(k, _, _)| *k == p.number).map(|(_, a, b)| (*a, *b)).collect(),
        })
        .collect();
    let log_warnings = log
        .iter()
        .filter(|l| {
            let l = l.to_ascii_lowercase();
            l.contains("areaportal") && (l.contains("doesn't touch") || l.contains("touches > 2") || l.contains("leak"))
        })
        .take(64)
        .cloned()
        .collect();
    let average_triangles =
        if heat.is_empty() { 0.0 } else { heat.iter().map(|h| f64::from(*h)).sum::<f64>() / heat.len() as f64 };
    VisReport {
        leaves: vis.leaf_count(),
        empty_leaves,
        clusters: cluster_count as u32,
        portals: portals.map_or(0, |p| p.portals.len()),
        areas: vis.area_count().saturating_sub(1).max(usize::from(cluster_count > 0)),
        area_stats: area_map.into_values().filter(|a| a.area > 0 || a.clusters > 0).collect(),
        has_vis,
        total_pvs,
        average_pvs: if cluster_count == 0 { 0.0 } else { total_pvs as f64 / cluster_count as f64 },
        max_pvs: visible.iter().copied().max().unwrap_or(0),
        vis_work,
        average_triangles,
        worst,
        small_clusters: small.len(),
        small_boxes: small.into_iter().take(MAX_SMALL_BOXES).collect(),
        areaportals,
        log_warnings,
    }
}

impl VisReport {
    pub fn findings(&self) -> Vec<Finding> {
        let mut out = Vec::new();
        if self.clusters == 0 {
            return out;
        }
        let n = self.clusters as f64;
        if self.has_vis && self.clusters >= OPEN_MAP_MIN && self.average_pvs > n * 0.6 {
            let pct = (self.average_pvs / n * 100.0).round();
            let impact = if self.clusters >= 200 && self.areas <= 1 { Impact::High } else { Impact::Medium };
            out.push(
                Finding::new(
                    Topic::Visleaves,
                    "vis_open_map",
                    impact,
                    format!(
                        "From an average visleaf you see {pct}% of the map ({:.0} of {} visleaves): almost nothing is culled. \
                         Split the map with walls that block the view, corners, func_detail for the small brushes, hint \
                         brushes and areaportals at the doorways.",
                        self.average_pvs, self.clusters
                    ),
                )
                .score(pct * 1000.0)
                .args(vec![format!("{pct}"), format!("{:.0}", self.average_pvs), self.clusters.to_string()]),
            );
        }
        for w in self.worst.iter().take(5) {
            if !self.has_vis || self.clusters < WORST_MIN || f64::from(w.visible) < (n * 0.5).max(6.0) {
                continue;
            }
            let center = w.bounds.center();
            out.push(
                Finding::new(
                    Topic::Visleaves,
                    "vis_worst_leaf",
                    if f64::from(w.visible) > n * 0.8 { Impact::Medium } else { Impact::Low },
                    format!(
                        "Visleaf {} near {} sees {} of {} visleaves ({} triangles drawn): block the view there (a wall, a \
                         corner) or add a hint brush along the edge of the opening so this leaf stops seeing the rest of \
                         the map.",
                        w.cluster,
                        fmt_point(center),
                        w.visible,
                        self.clusters,
                        w.triangles
                    ),
                )
                .score(f64::from(w.visible) * 100.0 + f64::from(w.triangles) * 0.01)
                .points(vec![center])
                .boxes(vec![w.bounds])
                .args(vec![
                    w.cluster.to_string(),
                    fmt_point(center),
                    w.visible.to_string(),
                    self.clusters.to_string(),
                    w.triangles.to_string(),
                ]),
            );
        }
        if self.small_clusters >= 4 && self.small_clusters as f64 > n * 0.15 {
            let center = self.small_boxes.first().map_or(DVec3::ZERO, Aabb::center);
            out.push(
                Finding::new(
                    Topic::Visleaves,
                    "vis_fragmented",
                    if self.small_clusters as f64 > n * 0.35 { Impact::High } else { Impact::Medium },
                    format!(
                        "{} of {} visleaves are tiny slivers: small world brushes (trims, steps, pillars, props made of \
                         brushes) cut the space. Turn them into func_detail (suggest_func_detail lists them).",
                        self.small_clusters, self.clusters
                    ),
                )
                .score(self.small_clusters as f64 * 50.0)
                .points(vec![center])
                .boxes(self.small_boxes.iter().take(16).copied().collect())
                .args(vec![self.small_clusters.to_string(), self.clusters.to_string()]),
            );
        }
        if self.portals > 0 && self.vis_work > 50_000_000 {
            out.push(
                Finding::new(
                    Topic::Visleaves,
                    "vis_slow",
                    Impact::Medium,
                    format!(
                        "VVIS has a lot of work ({} portals, estimated cost {}): fewer, simpler visleaves (func_detail, \
                         nodraw on hidden faces, a simpler hull) make the full visibility compile much faster.",
                        self.portals, self.vis_work
                    ),
                )
                .score((self.vis_work as f64).log10() * 100.0)
                .args(vec![self.portals.to_string(), self.vis_work.to_string()]),
            );
        }
        out
    }
}

pub fn portal_file(clusters: u32, portals: impl IntoIterator<Item = ([u32; 2], Vec<DVec3>)>) -> PortalFile {
    let mut bounds = Aabb::EMPTY;
    let list: Vec<Portal> = portals
        .into_iter()
        .filter(|(c, p)| p.len() >= 3 && c[0] < clusters && c[1] < clusters)
        .map(|(c, points)| {
            for p in &points {
                bounds.extend(*p);
            }
            Portal { clusters: c, points }
        })
        .collect();
    PortalFile { clusters, declared_portals: list.len(), portals: list, bounds }
}
