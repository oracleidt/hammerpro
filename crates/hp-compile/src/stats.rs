use serde::Serialize;

use crate::{LogLocation, Severity};

const COMPILE_BANNER: &str = "Hammer Pro map compiler (";
pub const HINTS_HEADER: &str = "---- Optimization hints ----";
const LIGHTMAP_MAX_BYTES: u64 = hp_bsp::limits::LIGHTING.sdk;
const LIGHTMAP_HINT_PERCENT: f64 = 90.0;

pub fn is_section_header(line: &str) -> bool {
    let t = line.trim();
    matches!(
        t,
        "---- Map statistics ----"
            | "---- Geometry details ----"
            | "---- Visibility statistics ----"
            | "---- Lighting statistics ----"
    ) || t == HINTS_HEADER
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LimitRow {
    pub name: String,
    pub used: Option<u64>,
    pub max: u64,
    pub bytes: u64,
    pub max_bytes: u64,
    pub percent: f64,
}

impl LimitRow {
    pub fn valve_name(&self) -> Option<&'static str> {
        valve_name(&self.name)
    }
}

pub fn valve_name(row: &str) -> Option<&'static str> {
    Some(match row.to_ascii_lowercase().as_str() {
        "models" => "MAX_MAP_MODELS",
        "brushes" => "MAX_MAP_BRUSHES",
        "brushsides" => "MAX_MAP_BRUSHSIDES",
        "planes" => "MAX_MAP_PLANES",
        "vertexes" => "MAX_MAP_VERTS",
        "nodes" => "MAX_MAP_NODES",
        "texinfo" => "MAX_MAP_TEXINFO",
        "texdata" => "MAX_MAP_TEXDATA",
        "dispinfo" => "MAX_MAP_DISPINFO",
        "disp_verts" => "MAX_MAP_DISP_VERTS",
        "disp_tris" => "MAX_MAP_DISP_TRIS",
        "faces" | "hdr faces" => "MAX_MAP_FACES",
        "origfaces" => "MAX_MAP_ORIGFACES",
        "leaves" => "MAX_MAP_LEAFS",
        "leaffaces" => "MAX_MAP_LEAFFACES",
        "leafbrushes" => "MAX_MAP_LEAFBRUSHES",
        "areas" => "MAX_MAP_AREAS",
        "areaportals" => "MAX_MAP_AREAPORTALS",
        "surfedges" => "MAX_MAP_SURFEDGES",
        "edges" => "MAX_MAP_EDGES",
        "leafwaterdata" => "MAX_MAP_LEAFWATERDATA",
        "cubemapsamples" => "MAX_MAP_CUBEMAPSAMPLES",
        "overlays" => "MAX_MAP_OVERLAYS",
        "occluders" => "MAX_MAP_OCCLUDERS",
        "entities" => "MAX_MAP_ENTITIES",
        "visdata" => "MAX_MAP_VISIBILITY",
        "entdata" => "MAX_MAP_ENTSTRING",
        "static props" => "MAX_STATIC_PROPS",
        "ldr worldlights" | "hdr worldlights" => "MAX_MAP_WORLDLIGHTS",
        "ldr lightdata" | "hdr lightdata" => "dface_t lightofs",
        "ldr leaf ambient" | "hdr leaf ambient" => "MAX_MAP_LEAFS",
        _ => return None,
    })
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Timing {
    pub name: String,
    pub secs: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct NameCount {
    pub name: String,
    pub count: u64,
    pub percent: Option<f64>,
    pub brushes: Option<u64>,
    pub position: Option<[f64; 3]>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct BrushCounts {
    pub total: u64,
    pub structural: u64,
    pub detail: u64,
    pub water: u64,
    pub tool: u64,
    pub entity: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct AreaStat {
    pub area: u32,
    pub leaves: u64,
    pub clusters: u64,
    pub faces: u64,
    pub position: Option<[f64; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct SmallBrush {
    pub brush_id: u32,
    pub size: [f64; 3],
    pub position: Option<[f64; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct GeometryStats {
    pub brushes: BrushCounts,
    pub entities_in_map: u64,
    pub entities_in_lump: u64,
    pub brush_entities: u64,
    pub point_entities: u64,
    pub world_faces: u64,
    pub entity_faces: u64,
    pub displacement_faces: u64,
    pub triangles: u64,
    pub displacements: u64,
    pub displacement_powers: [u64; 3],
    pub map_size: Option<[f64; 3]>,
    pub bsp_bytes: Option<u64>,
    pub estimated_luxels: Option<u64>,
    pub estimated_lightmap_bytes: Option<u64>,
    pub max_lightmap_bytes: u64,
    pub materials: Vec<NameCount>,
    pub material_total: u64,
    pub entity_classes: Vec<NameCount>,
    pub brush_entity_classes: Vec<NameCount>,
    pub areas: Vec<AreaStat>,
    pub area_total: u64,
    pub small_brushes: Vec<SmallBrush>,
    pub small_brush_total: u64,
    pub fadeless_props: Vec<NameCount>,
    pub fadeless_total: u64,
    pub stages: Vec<Timing>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ClusterStat {
    pub cluster: u32,
    pub sees: u64,
    pub percent: f64,
    pub faces: u64,
    pub position: Option<[f64; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct VisStats {
    pub clusters: u64,
    pub portals: u64,
    pub leaves: u64,
    pub mode: String,
    pub reused: bool,
    pub radius: Option<f64>,
    pub radius_source: Option<String>,
    pub average: f64,
    pub average_percent: f64,
    pub max: u64,
    pub max_percent: f64,
    pub times: Vec<Timing>,
    pub worst: Vec<ClusterStat>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct PassBytes {
    pub pass: String,
    pub bytes: u64,
    pub max_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct LightCounts {
    pub total: u64,
    pub point: u64,
    pub spot: u64,
    pub sun: u64,
    pub sky_ambient: u64,
    pub surface: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LuxelFace {
    pub luxels: u64,
    pub scale: f64,
    pub side_id: u32,
    pub size: [f64; 3],
    pub material: String,
    pub position: Option<[f64; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct LightingStats {
    pub faces: u64,
    pub lit_faces: u64,
    pub luxels: u64,
    pub passes: Vec<PassBytes>,
    pub lights: LightCounts,
    pub switchable_styles: u64,
    pub max_switchable_styles: u64,
    pub styles: u64,
    pub texture_light_faces: u64,
    pub static_props: u64,
    pub vertex_lit_props: u64,
    pub detail_props: u64,
    pub detail_lit: u64,
    pub bounces: u64,
    pub ambient_samples: u64,
    pub ambient_leaves: u64,
    pub stages: Vec<Timing>,
    pub top_faces: Vec<LuxelFace>,
    pub top_materials: Vec<NameCount>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CompileStats {
    pub limits: Vec<LimitRow>,
    pub geometry: Option<GeometryStats>,
    pub vis: Option<VisStats>,
    pub lighting: Option<LightingStats>,
}

impl CompileStats {
    pub fn is_empty(&self) -> bool {
        self.limits.is_empty() && self.geometry.is_none() && self.vis.is_none() && self.lighting.is_none()
    }

    pub fn limit(&self, name: &str) -> Option<&LimitRow> {
        self.limits.iter().find(|r| r.name.eq_ignore_ascii_case(name))
    }
}

fn number(tok: &str) -> Option<f64> {
    let t = tok.trim_matches(|c: char| matches!(c, '(' | ')' | ',' | ':' | '%' | ';'));
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

fn after(s: &str, label: &str) -> Option<f64> {
    let at = s.find(label)? + label.len();
    s[at..].split_whitespace().next().and_then(number)
}

fn before(s: &str, label: &str) -> Option<f64> {
    let at = s.find(label)?;
    s[..at].split_whitespace().next_back().and_then(number)
}

fn int(v: Option<f64>) -> u64 {
    v.map_or(0, |v| v.max(0.0) as u64)
}

fn position(s: &str) -> Option<[f64; 3]> {
    let at = s.rfind(" at (")? + " at (".len();
    let inner = s[at..].split(')').next()?;
    let v: Vec<f64> = inner.split_whitespace().filter_map(number).collect();
    (v.len() == 3).then(|| [v[0], v[1], v[2]])
}

fn size3(s: &str) -> Option<[f64; 3]> {
    let end = s.find(" units")?;
    let toks: Vec<&str> = s[..end].split_whitespace().collect();
    let n = toks.len();
    if n < 5 || toks[n - 2] != "x" || toks[n - 4] != "x" {
        return None;
    }
    Some([number(toks[n - 5])?, number(toks[n - 3])?, number(toks[n - 1])?])
}

fn name_between(s: &str, start: usize) -> String {
    let end = s.rfind(" at (").unwrap_or(s.len()).max(start);
    s[start..end].trim().to_owned()
}

pub fn parse_limit_row(line: &str) -> Option<LimitRow> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    let pair = |t: &str| -> Option<(u64, u64)> {
        let (a, b) = t.split_once('/')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    };
    let k = toks.iter().position(|t| *t == "[variable]" || pair(t).is_some())?;
    if k == 0 {
        return None;
    }
    let name = toks[..k].join(" ");
    let (used, max) = if toks[k] == "[variable]" { (None, 0) } else { pair(toks[k]).map(|(u, m)| (Some(u), m))? };
    let (bytes, max_bytes) = pair(toks.get(k + 1)?)?;
    let rest = toks[k + 2..].join("");
    let percent = rest.strip_prefix('(')?.strip_suffix(")")?.strip_suffix('%')?.trim().parse::<f64>().ok()?;
    Some(LimitRow { name, used, max, bytes, max_bytes, percent })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Section {
    None,
    Geometry,
    Details,
    Vis,
    Lighting,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum List {
    None,
    GeometryStages,
    Materials,
    Classes,
    BrushClasses,
    Areas,
    SmallBrushes,
    Fadeless,
    Worst,
    LightingStages,
    LuxelFaces,
    LuxelMaterials,
}

pub fn parse<'a>(lines: impl IntoIterator<Item = &'a str>) -> CompileStats {
    let mut st = CompileStats::default();
    let mut section = Section::None;
    let mut list = List::None;
    for raw in lines {
        let line = raw.trim_end();
        let t = line.trim_start();
        if t.starts_with(COMPILE_BANNER) {
            st = CompileStats::default();
            section = Section::None;
            list = List::None;
            continue;
        }
        if t.starts_with("---- ") && t.ends_with(" ----") {
            list = List::None;
            section = match t {
                "---- Map statistics ----" => {
                    st.geometry.get_or_insert_with(GeometryStats::default);
                    Section::Geometry
                }
                "---- Geometry details ----" => {
                    st.geometry = Some(GeometryStats::default());
                    Section::Details
                }
                "---- Visibility statistics ----" => {
                    st.vis = Some(VisStats::default());
                    Section::Vis
                }
                "---- Lighting statistics ----" => {
                    st.lighting = Some(LightingStats::default());
                    Section::Lighting
                }
                _ => Section::None,
            };
            continue;
        }
        if section == Section::None {
            continue;
        }
        if matches!(section, Section::Geometry | Section::Lighting)
            && let Some(row) = parse_limit_row(t)
        {
            match st.limits.iter_mut().find(|r| r.name == row.name) {
                Some(r) => *r = row,
                None => st.limits.push(row),
            }
            continue;
        }
        if let Some(item) = t.strip_prefix('#') {
            let Some((rank, rest)) = item.split_once(char::is_whitespace) else { continue };
            if rank.parse::<u32>().is_err() {
                continue;
            }
            parse_item(&mut st, list, rest.trim());
            continue;
        }
        list = List::None;
        if t.ends_with(':') {
            list = list_of(section, t);
            continue;
        }
        match section {
            Section::Details => {
                if let Some(g) = st.geometry.as_mut() {
                    parse_geometry_line(g, t);
                }
            }
            Section::Vis => {
                if let Some(v) = st.vis.as_mut() {
                    parse_vis_line(v, t);
                }
            }
            Section::Lighting => {
                if let Some(l) = st.lighting.as_mut() {
                    parse_lighting_line(l, t);
                }
            }
            Section::Geometry | Section::None => {}
        }
    }
    st
}

fn list_of(section: Section, header: &str) -> List {
    let starts = |p: &str| header.starts_with(p);
    match section {
        Section::Details if starts("Stage times") => List::GeometryStages,
        Section::Details if starts("Materials by faces") => List::Materials,
        Section::Details if starts("Entities by class") => List::Classes,
        Section::Details if starts("Brush entities by class") => List::BrushClasses,
        Section::Details if starts("Areas by faces") => List::Areas,
        Section::Details if starts("Small structural brushes") => List::SmallBrushes,
        Section::Details if starts("Static props without fade distance") => List::Fadeless,
        Section::Vis if starts("Clusters that see the most") => List::Worst,
        Section::Lighting if starts("Stage times") => List::LightingStages,
        Section::Lighting if starts("Faces with the most luxels") => List::LuxelFaces,
        Section::Lighting if starts("Materials with the most luxels") => List::LuxelMaterials,
        _ => List::None,
    }
}

fn timing(rest: &str) -> Option<Timing> {
    let mut toks = rest.splitn(3, char::is_whitespace);
    let secs = number(toks.next()?)?;
    let rest = toks.next().filter(|s| *s == "s").and(toks.next())?;
    Some(Timing { name: rest.trim().to_owned(), secs })
}

fn parse_item(st: &mut CompileStats, list: List, rest: &str) {
    let toks: Vec<&str> = rest.split_whitespace().collect();
    let tok = |i: usize| toks.get(i).copied().unwrap_or("");
    let g = || NameCount::default();
    match list {
        List::GeometryStages => {
            if let (Some(geo), Some(t)) = (st.geometry.as_mut(), timing(rest)) {
                geo.stages.push(t);
            }
        }
        List::LightingStages => {
            if let (Some(l), Some(t)) = (st.lighting.as_mut(), timing(rest)) {
                l.stages.push(t);
            }
        }
        List::Materials => {
            if let Some(geo) = st.geometry.as_mut() {
                let name = toks.get(3..).map(|t| t.join(" ")).unwrap_or_default();
                geo.materials.push(NameCount { name, count: int(number(tok(0))), percent: number(tok(2)), ..g() });
            }
        }
        List::Classes => {
            if let Some(geo) = st.geometry.as_mut() {
                geo.entity_classes.push(NameCount { name: tok(1).to_owned(), count: int(number(tok(0))), ..g() });
            }
        }
        List::BrushClasses => {
            if let Some(geo) = st.geometry.as_mut() {
                geo.brush_entity_classes.push(NameCount {
                    name: tok(1).to_owned(),
                    count: int(number(tok(0))),
                    brushes: number(tok(2)).map(|v| v as u64),
                    ..g()
                });
            }
        }
        List::Areas => {
            if let Some(geo) = st.geometry.as_mut() {
                geo.areas.push(AreaStat {
                    area: int(after(rest, "area ")) as u32,
                    leaves: int(before(rest, " leaves")),
                    clusters: int(before(rest, " clusters")),
                    faces: int(before(rest, " faces")),
                    position: position(rest),
                });
            }
        }
        List::SmallBrushes => {
            if let Some(geo) = st.geometry.as_mut() {
                geo.small_brushes.push(SmallBrush {
                    brush_id: int(after(rest, "brush ")) as u32,
                    size: size3(rest).unwrap_or_default(),
                    position: position(rest),
                });
            }
        }
        List::Fadeless => {
            if let Some(geo) = st.geometry.as_mut() {
                let start = rest.find(" props").map_or(rest.len(), |p| p + " props".len());
                geo.fadeless_props.push(NameCount {
                    name: name_between(rest, start),
                    count: int(number(tok(0))),
                    position: position(rest),
                    ..g()
                });
            }
        }
        List::Worst => {
            if let Some(v) = st.vis.as_mut() {
                v.worst.push(ClusterStat {
                    cluster: int(after(rest, "cluster ")) as u32,
                    sees: int(after(rest, "sees ")),
                    percent: after(rest, "clusters ").unwrap_or(0.0),
                    faces: int(before(rest, " faces")),
                    position: position(rest),
                });
            }
        }
        List::LuxelFaces => {
            if let Some(l) = st.lighting.as_mut() {
                let start = rest.find(" units").map_or(rest.len(), |p| p + " units".len());
                l.top_faces.push(LuxelFace {
                    luxels: int(number(tok(0))),
                    scale: after(rest, "scale ").unwrap_or(0.0),
                    side_id: int(after(rest, "side ")) as u32,
                    size: size3(rest).unwrap_or_default(),
                    material: name_between(rest, start),
                    position: position(rest),
                });
            }
        }
        List::LuxelMaterials => {
            if let Some(l) = st.lighting.as_mut() {
                let start = rest.find(" faces").map_or(rest.len(), |p| p + " faces".len());
                l.top_materials.push(NameCount {
                    name: rest[start..].trim().to_owned(),
                    count: int(number(tok(0))),
                    percent: number(tok(2)),
                    brushes: Some(int(before(rest, " faces"))),
                    ..g()
                });
            }
        }
        List::None => {}
    }
}

fn parse_geometry_line(g: &mut GeometryStats, t: &str) {
    if let Some(s) = t.strip_prefix("Brushes:") {
        g.brushes = BrushCounts {
            total: int(before(s, " total")),
            structural: int(before(s, " structural")),
            detail: int(before(s, " detail")),
            water: int(before(s, " water")),
            tool: int(before(s, " tool")),
            entity: int(before(s, " in brush entities")),
        };
    } else if let Some(s) = t.strip_prefix("Entities:") {
        g.entities_in_map = int(before(s, " in the map"));
        g.entities_in_lump = int(before(s, " in the entity lump"));
        g.brush_entities = int(before(s, " brush entities"));
        g.point_entities = int(before(s, " point entities"));
    } else if let Some(s) = t.strip_prefix("Faces:") {
        g.world_faces = int(before(s, " world"));
        g.entity_faces = int(before(s, " in brush entities"));
        g.displacement_faces = int(before(s, " displacements"));
        g.triangles = int(before(s, " triangles"));
    } else if let Some(s) = t.strip_prefix("Displacements:") {
        g.displacements = int(after(s, ""));
        g.displacement_powers = [int(after(s, "power 2:")), int(after(s, "power 3:")), int(after(s, "power 4:"))];
    } else if let Some(s) = t.strip_prefix("Map size:") {
        g.map_size = size3(s);
    } else if let Some(s) = t.strip_prefix("BSP file:") {
        g.bsp_bytes = after(s, "").map(|v| v as u64);
    } else if let Some(s) = t.strip_prefix("Lightmaps (estimate):") {
        g.estimated_luxels = before(s, " luxels").map(|v| v as u64);
        g.estimated_lightmap_bytes = before(s, " bytes per pass").map(|v| v as u64);
        g.max_lightmap_bytes = int(after(s, "per pass of"));
    }
}

fn parse_vis_line(v: &mut VisStats, t: &str) {
    if let Some(s) = t.strip_prefix("Clusters:") {
        v.clusters = int(after(s, ""));
        v.portals = int(after(s, "portals:"));
        v.leaves = int(after(s, "leaves:"));
    } else if let Some(s) = t.strip_prefix("Vis mode:") {
        let s = s.trim();
        let (mode, reused) = s.strip_suffix("(reused)").map_or((s, false), |m| (m.trim_end(), true));
        v.mode = mode.to_owned();
        v.reused = reused;
    } else if let Some(s) = t.strip_prefix("Vis radius:") {
        let s = s.trim();
        v.radius = s.split_whitespace().next().and_then(number);
        v.radius_source = s.split_once('(').map(|(_, r)| r.trim_end_matches(')').to_owned());
    } else if let Some(s) = t.strip_prefix("Vis times:") {
        v.times = s
            .split(',')
            .filter_map(|part| {
                let toks: Vec<&str> = part.split_whitespace().collect();
                let (name, secs) = (toks.first()?, number(toks.get(1)?)?);
                (*name != "total").then(|| Timing { name: (*name).to_owned(), secs })
            })
            .collect();
    } else if let Some(s) = t.strip_prefix("Visible clusters:") {
        v.average = after(s, "average").unwrap_or(0.0);
        v.average_percent =
            after(s, "average").and_then(|_| s.split_whitespace().nth(2)).and_then(number).unwrap_or(0.0);
        v.max = int(after(s, "max"));
        v.max_percent =
            s.split_once("max").and_then(|(_, r)| r.split_whitespace().nth(1)).and_then(number).unwrap_or(0.0);
    }
}

fn parse_lighting_line(l: &mut LightingStats, t: &str) {
    if let Some(s) = t.strip_prefix("Faces:") {
        l.faces = int(before(s, " faces"));
        l.lit_faces = int(before(s, " with lightmaps"));
        l.luxels = int(before(s, " luxels"));
    } else if let Some(s) = t.strip_prefix("Lightmap bytes:") {
        let mut toks = s.split_whitespace();
        let pass = toks.next().unwrap_or("").to_owned();
        let bytes = int(toks.next().and_then(number));
        l.passes.push(PassBytes { pass, bytes, max_bytes: int(after(s, " of")) });
    } else if let Some(s) = t.strip_prefix("Lights:") {
        l.lights = LightCounts {
            total: int(before(s, " total")),
            point: int(before(s, " point")),
            spot: int(before(s, " spot")),
            sun: int(before(s, " sun")),
            sky_ambient: int(before(s, " sky ambient")),
            surface: int(before(s, " surface")),
        };
    } else if let Some(s) = t.strip_prefix("Light styles:") {
        l.switchable_styles = int(before(s, " switchable"));
        l.max_switchable_styles = int(after(s, "switchable of"));
        l.styles = int(before(s, " styles used"));
    } else if let Some(s) = t.strip_prefix("Texture lights:") {
        l.texture_light_faces = int(before(s, " faces"));
    } else if let Some(s) = t.strip_prefix("Static props:") {
        l.static_props = int(after(s, ""));
        l.vertex_lit_props = int(before(s, " with vertex lighting"));
    } else if let Some(s) = t.strip_prefix("Detail props:") {
        l.detail_props = int(after(s, ""));
        l.detail_lit = int(before(s, " lit"));
    } else if let Some(s) = t.strip_prefix("Bounces:") {
        l.bounces = int(after(s, ""));
    } else if let Some(s) = t.strip_prefix("Leaf ambient:") {
        l.ambient_samples = int(before(s, " samples"));
        l.ambient_leaves = int(before(s, " leaves"));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HintLevel {
    Tip,
    Important,
    Critical,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HintKind {
    NearLimit { name: String, valve: String, percent: f64 },
    WideView { cluster: u32, percent: f64, faces: u64 },
    WeakVis { average_percent: f64 },
    NoVis { clusters: u64 },
    NoVisWater { water: u64 },
    NoAreaportals { clusters: u64 },
    SmallBrushes { count: u64, brush_id: u32 },
    FadelessProps { count: u64, model: String, of_model: u64 },
    Power4Displacements { count: u64 },
    LightmapEstimate { percent: f64 },
    LightmapBytes { pass: String, percent: f64 },
    FineLightmap { luxels: u64, scale: f64, material: String },
    SwitchableStyles { used: u64, max: u64 },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Hint {
    #[serde(flatten)]
    pub kind: HintKind,
    pub level: HintLevel,
    pub location: Option<LogLocation>,
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

impl Hint {
    pub fn key(&self) -> &'static str {
        match self.kind {
            HintKind::NearLimit { percent, .. } if percent > 100.0 => "compilestats.hint.over_limit",
            HintKind::NearLimit { .. } => "compilestats.hint.near_limit",
            HintKind::WideView { .. } => "compilestats.hint.wide_view",
            HintKind::WeakVis { .. } => "compilestats.hint.weak_vis",
            HintKind::NoVis { .. } => "compilestats.hint.no_vis",
            HintKind::NoVisWater { .. } => "compilestats.hint.no_vis_water",
            HintKind::NoAreaportals { .. } => "compilestats.hint.no_areaportals",
            HintKind::SmallBrushes { .. } => "compilestats.hint.small_brushes",
            HintKind::FadelessProps { .. } => "compilestats.hint.fadeless_props",
            HintKind::Power4Displacements { .. } => "compilestats.hint.power4",
            HintKind::LightmapEstimate { .. } => "compilestats.hint.lightmap_estimate",
            HintKind::LightmapBytes { .. } => "compilestats.hint.lightmap_bytes",
            HintKind::FineLightmap { .. } => "compilestats.hint.fine_lightmap",
            HintKind::SwitchableStyles { .. } => "compilestats.hint.switchable_styles",
        }
    }

    pub fn args(&self) -> Vec<String> {
        match &self.kind {
            HintKind::NearLimit { name, valve, percent } => {
                vec![name.clone(), format!("{:.1}", percent), valve.clone()]
            }
            HintKind::WideView { cluster, percent, faces } => {
                vec![cluster.to_string(), format!("{percent:.0}"), faces.to_string()]
            }
            HintKind::WeakVis { average_percent } => vec![format!("{average_percent:.0}")],
            HintKind::NoVisWater { water } => vec![water.to_string()],
            HintKind::NoVis { clusters } | HintKind::NoAreaportals { clusters } => vec![clusters.to_string()],
            HintKind::SmallBrushes { count, brush_id } => vec![count.to_string(), brush_id.to_string()],
            HintKind::FadelessProps { count, model, of_model } => {
                vec![count.to_string(), model.clone(), of_model.to_string()]
            }
            HintKind::Power4Displacements { count } => vec![count.to_string()],
            HintKind::LightmapEstimate { percent } => vec![format!("{percent:.0}")],
            HintKind::LightmapBytes { pass, percent } => vec![pass.clone(), format!("{percent:.0}")],
            HintKind::FineLightmap { luxels, scale, material } => {
                vec![luxels.to_string(), round1(*scale).to_string(), material.clone()]
            }
            HintKind::SwitchableStyles { used, max } => vec![used.to_string(), max.to_string()],
        }
    }

    pub fn english(&self) -> String {
        let template = HINT_TEXTS.iter().find(|(k, _)| *k == self.key()).map_or("", |(_, t)| *t);
        let args = self.args();
        let mut out = template.to_owned();
        for (i, a) in args.iter().enumerate() {
            out = out.replace(&format!("{{{i}}}"), a);
        }
        out
    }
}

pub const HINT_TEXTS: &[(&str, &str)] = &[
    ("compilestats.hint.near_limit", "{0} uses {1}% of the engine limit ({2}): reduce it before it stops the compile."),
    (
        "compilestats.hint.over_limit",
        "{0} is over the engine limit at {1}% ({2}): the game may refuse the map, bring it under the limit.",
    ),
    (
        "compilestats.hint.wide_view",
        "Cluster {0} sees {1}% of the map ({2} faces drawn from there): hint brushes, areaportals or turning the open details into func_detail there cut what is drawn.",
    ),
    (
        "compilestats.hint.weak_vis",
        "Each cluster sees {0}% of the map on average: the visibility hides little; split the map with walls, corners, hint brushes and areaportals.",
    ),
    (
        "compilestats.hint.no_vis",
        "The visibility was not computed: the {0} clusters all draw everything. Compile with the visibility to judge the frame rate.",
    ),
    (
        "compilestats.hint.no_vis_water",
        "Water brushes: {0}, and no visibility: in game the water draws black (no refraction). Compile with the visibility, fast is enough.",
    ),
    (
        "compilestats.hint.no_areaportals",
        "{0} clusters and no areaportal: func_areaportal in the doorways close off whole rooms.",
    ),
    (
        "compilestats.hint.small_brushes",
        "{0} small structural brushes (pillars, trims, steps) cut the visibility for nothing: make them func_detail (the smallest is brush {1}).",
    ),
    (
        "compilestats.hint.fadeless_props",
        "{0} static props have no fade distance ({2} of {1}): a fade distance keeps far props from being drawn.",
    ),
    (
        "compilestats.hint.power4",
        "{0} displacements of power 4: power 3 is usually enough and keeps the collision mesh faster.",
    ),
    (
        "compilestats.hint.lightmap_estimate",
        "The lightmaps will take about {0}% of the 2 GB a face can address in the lightmaps (past it the game cannot load the map): raise the lightmap scale of large faces.",
    ),
    (
        "compilestats.hint.lightmap_bytes",
        "The {0} lightmaps take {1}% of the 2 GB a face can address in the lightmaps (past it the game cannot load the map): raise the lightmap scale of large faces.",
    ),
    (
        "compilestats.hint.fine_lightmap",
        "A face of {2} takes {0} luxels at lightmap scale {1}: a larger scale (16 or more) saves lightmap memory where shadows stay soft.",
    ),
    (
        "compilestats.hint.switchable_styles",
        "{0} of the {1} switchable light styles are used: named lights that never switch need no name.",
    ),
];

pub fn hints(st: &CompileStats) -> Vec<Hint> {
    let mut out = Vec::new();
    let at = |p: Option<[f64; 3]>| p.map(|p| LogLocation { position: Some(p), ..LogLocation::default() });
    for r in &st.limits {
        if r.percent >= 75.0 && !r.name.to_ascii_lowercase().ends_with("lightdata") {
            let level = if r.percent > 100.0 {
                HintLevel::Critical
            } else if r.percent >= 90.0 {
                HintLevel::Important
            } else {
                HintLevel::Tip
            };
            out.push(Hint {
                kind: HintKind::NearLimit {
                    name: r.name.clone(),
                    valve: r.valve_name().unwrap_or("").to_owned(),
                    percent: r.percent,
                },
                level,
                location: None,
            });
        }
    }
    if let Some(v) = &st.vis {
        let water = st.geometry.as_ref().map_or(0, |g| g.brushes.water);
        if v.mode == "off" && water > 0 {
            out.push(Hint { kind: HintKind::NoVisWater { water }, level: HintLevel::Critical, location: None });
        }
        if v.mode == "off" && v.clusters >= 2 {
            out.push(Hint { kind: HintKind::NoVis { clusters: v.clusters }, level: HintLevel::Tip, location: None });
        }
        if v.clusters >= 20 {
            for c in v.worst.iter().filter(|c| c.percent >= 50.0).take(3) {
                out.push(Hint {
                    kind: HintKind::WideView { cluster: c.cluster, percent: c.percent, faces: c.faces },
                    level: HintLevel::Important,
                    location: at(c.position),
                });
            }
        }
        if v.clusters >= 100 && v.average_percent >= 35.0 {
            out.push(Hint {
                kind: HintKind::WeakVis { average_percent: v.average_percent },
                level: HintLevel::Important,
                location: None,
            });
        }
        let areaportals = st.limit("areaportals").and_then(|r| r.used).unwrap_or(0);
        if v.clusters >= 300 && areaportals <= 1 {
            out.push(Hint {
                kind: HintKind::NoAreaportals { clusters: v.clusters },
                level: HintLevel::Tip,
                location: None,
            });
        }
    }
    if let Some(g) = &st.geometry {
        if g.small_brush_total >= 20
            && let Some(b) = g.small_brushes.first()
        {
            out.push(Hint {
                kind: HintKind::SmallBrushes { count: g.small_brush_total, brush_id: b.brush_id },
                level: HintLevel::Tip,
                location: Some(LogLocation {
                    brush_id: Some(b.brush_id),
                    position: b.position,
                    ..LogLocation::default()
                }),
            });
        }
        if g.fadeless_total >= 30
            && let Some(p) = g.fadeless_props.first()
        {
            out.push(Hint {
                kind: HintKind::FadelessProps { count: g.fadeless_total, model: p.name.clone(), of_model: p.count },
                level: HintLevel::Tip,
                location: at(p.position),
            });
        }
        if g.displacement_powers[2] > 0 {
            out.push(Hint {
                kind: HintKind::Power4Displacements { count: g.displacement_powers[2] },
                level: HintLevel::Tip,
                location: None,
            });
        }
        if st.lighting.is_none()
            && let Some(b) = g.estimated_lightmap_bytes
        {
            let percent = b as f64 * 100.0 / LIGHTMAP_MAX_BYTES as f64;
            if percent >= LIGHTMAP_HINT_PERCENT {
                let level = if percent > 100.0 { HintLevel::Critical } else { HintLevel::Important };
                out.push(Hint { kind: HintKind::LightmapEstimate { percent }, level, location: None });
            }
        }
    }
    if let Some(l) = &st.lighting {
        for p in &l.passes {
            let percent = p.bytes as f64 * 100.0 / LIGHTMAP_MAX_BYTES as f64;
            if percent >= LIGHTMAP_HINT_PERCENT {
                let level = if percent > 100.0 { HintLevel::Critical } else { HintLevel::Important };
                out.push(Hint {
                    kind: HintKind::LightmapBytes { pass: p.pass.clone(), percent },
                    level,
                    location: None,
                });
            }
        }
        for f in l.top_faces.iter().filter(|f| f.scale > 0.0 && f.scale < 16.0 && f.luxels >= 8192).take(3) {
            out.push(Hint {
                kind: HintKind::FineLightmap { luxels: f.luxels, scale: f.scale, material: f.material.clone() },
                level: HintLevel::Tip,
                location: Some(LogLocation {
                    side_id: (f.side_id > 0).then_some(f.side_id),
                    position: f.position,
                    ..LogLocation::default()
                }),
            });
        }
        if l.max_switchable_styles > 0 && l.switchable_styles * 3 >= l.max_switchable_styles * 2 {
            out.push(Hint {
                kind: HintKind::SwitchableStyles { used: l.switchable_styles, max: l.max_switchable_styles },
                level: if l.switchable_styles >= l.max_switchable_styles {
                    HintLevel::Important
                } else {
                    HintLevel::Tip
                },
                location: None,
            });
        }
    }
    out.sort_by_key(|h| std::cmp::Reverse(h.level));
    out
}

pub fn hint_lines(st: &CompileStats) -> Vec<String> {
    let list = hints(st);
    if list.is_empty() {
        return Vec::new();
    }
    let mut out = vec![HINTS_HEADER.to_owned()];
    for h in list {
        let place = h
            .location
            .as_ref()
            .and_then(|l| l.position)
            .map(|p| format!(" (at {:.0} {:.0} {:.0})", p[0], p[1], p[2]))
            .unwrap_or_default();
        out.push(format!("Hint: {}{place}", h.english()));
    }
    out
}

pub fn hint_severity(level: HintLevel) -> Severity {
    match level {
        HintLevel::Tip => Severity::Info,
        HintLevel::Important => Severity::Warning,
        HintLevel::Critical => Severity::Error,
    }
}

fn totals(st: &mut CompileStats, lines: &[&str]) {
    let Some(g) = st.geometry.as_mut() else { return };
    for l in lines {
        let t = l.trim();
        let total =
            || t.split_once(" of ").and_then(|(_, r)| r.split_whitespace().next()).and_then(number).map(|v| v as u64);
        if t.starts_with("Small structural brushes (") {
            g.small_brush_total = total().unwrap_or(g.small_brushes.len() as u64);
        } else if t.starts_with("Static props without fade distance (") {
            g.fadeless_total = total().unwrap_or(0);
        } else if t.starts_with("Materials by faces (") {
            g.material_total = total().unwrap_or(0);
        } else if t.starts_with("Areas by faces (") {
            g.area_total = total().unwrap_or(0);
        }
    }
}

pub fn parse_text(text: &str) -> CompileStats {
    let lines: Vec<&str> = text.lines().collect();
    parse_lines(&lines)
}

pub fn parse_lines(lines: &[&str]) -> CompileStats {
    let mut st = parse(lines.iter().copied());
    let from = lines.iter().rposition(|l| l.trim_start().starts_with(COMPILE_BANNER)).unwrap_or(0);
    let geometry_from =
        lines[from..].iter().rposition(|l| l.trim() == "---- Geometry details ----").map_or(from, |p| from + p);
    totals(&mut st, &lines[geometry_from..]);
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "\
Hammer Pro map compiler (hp-bspc 0.1.0)
---- Map statistics ----
models  1/1024  48/49152  (0.1%)
Hammer Pro map compiler (hp-bspc 0.1.0)
Command line: --bspc -vis gm_x
BSP checked: every index and range is valid
---- Map statistics ----
Object names            Objects/Maxobjs       Memory / Maxmem   Fullness
models                             2/1024             96/49152  (  0.2%)
brushsides                   60000/65536      480000/524288  ( 91.6%)
areaportals                          1/1024             12/12288  (  0.1%)
visdata                       [variable]        1234/16777216  (  0.0%)
Warning: brushsides uses 91.6% of the engine limit (MAX_MAP_BRUSHSIDES = 65536)
---- Geometry details ----
Brushes: 40 total, 30 structural, 5 detail, 1 water, 2 tool, 2 in brush entities
Entities: 12 in the map, 8 in the entity lump, 3 brush entities, 9 point entities
Faces: 300 world, 20 in brush entities, 4 displacements, 900 triangles
Displacements: 4 (power 2: 1, power 3: 2, power 4: 1)
Map size: 2048 x 1024 x 512 units
BSP file: 123456 bytes
Lightmaps (estimate): 50000 luxels, 14000000 bytes per pass of 16777216 (83.4%)
Materials by faces (top 2 of 7):
#1  200 faces  62.5%  dev/dev_measurewall01a
#2  50 faces  15.6%  tools/toolsnodraw
Entities by class (top 2 of 5):
#1  40  prop_static
#2  3  func_door
Brush entities by class (top 1 of 1):
#1  3  func_door  6 brushes
Areas by faces (top 1 of 2):
#1  area 1  100 leaves  40 clusters  280 faces  at (128 -64 32)
Small structural brushes (top 2 of 25):
#1  brush 77  16 x 16 x 16 units  at (8 8 8)
#2  brush 78  32 x 8 x 128 units  at (100 0 64)
Static props without fade distance (top 1 of 35 props):
#1  35 props  models/props c17/oildrum001.mdl  at (1 2 3)
Stage times (geometry):
#1  0.120 s  read brushes
#2  1.500 s  final tree
---- Visibility statistics ----
   120 portalclusters
   340 numportals
Clusters: 120, portals: 340, leaves: 800 (500 empty)
Vis mode: full
Vis radius: 4096 (env_fog_controller farz)
Vis times: base 0.100 s, flow 2.000 s, pas 0.050 s, total 2.150 s
Visible clusters: average 48.0 (40.0%), max 90 (75.0%) in cluster 7
Clusters that see the most (top 2 of 120):
#1  cluster 7  sees 90 clusters (75.0%)  2500 faces  at (10 20 30)
#2  cluster 8  sees 30 clusters (25.0%)  900 faces  at (-1 -2 -3)
Hint: something
Hammer Pro lighting (hp-bspc 0.1.0)
---- Lighting statistics ----
Object names            Objects/Maxobjs       Memory / Maxmem   Fullness
LDR worldlights                     12/8192           1056/720896  (  0.1%)
LDR lightdata                 [variable]  13000000/16777216  ( 77.5%)
Faces: 320 faces, 300 with lightmaps, 70000 luxels
Lightmap bytes: LDR 13000000 of 16777216 (77.5%)
Lights: 12 total, 8 point, 2 spot, 1 sun, 1 sky ambient, 0 surface
Light styles: 70 switchable of 96, 72 styles used
Static props: 40, 38 with vertex lighting
Detail props: 100, 100 lit
Bounces: 12
Passes: LDR
Leaf ambient: 900 samples in 450 leaves
Stage times (lighting):
#1  0.500 s  scene
#2  4.000 s  direct light
Faces with the most luxels (top 2 of 300):
#1  20000 luxels  scale 4  side 1234  512 x 0 x 512 units  concrete/wall 01  at (0 256 128)
#2  900 luxels  scale 16  side 9  64 x 64 x 0 units  dev/floor  at (5 5 0)
Materials with the most luxels (top 1 of 3):
#1  60000 luxels  85.7%  250 faces  concrete/wall01
";

    #[test]
    fn the_log_is_read_back() {
        let st = parse_text(LOG);
        assert_eq!(st.limits.len(), 6, "{:#?}", st.limits);
        let models = st.limit("models").unwrap();
        assert_eq!((models.used, models.max, models.bytes, models.percent), (Some(2), 1024, 96, 0.2));
        let vis = st.limit("visdata").unwrap();
        assert_eq!((vis.used, vis.bytes, vis.max_bytes), (None, 1234, 16_777_216));
        assert_eq!(st.limit("LDR lightdata").unwrap().percent, 77.5);
        assert_eq!(st.limit("brushsides").unwrap().valve_name(), Some("MAX_MAP_BRUSHSIDES"));

        let g = st.geometry.as_ref().unwrap();
        assert_eq!(g.brushes, BrushCounts { total: 40, structural: 30, detail: 5, water: 1, tool: 2, entity: 2 });
        assert_eq!((g.entities_in_map, g.entities_in_lump, g.brush_entities, g.point_entities), (12, 8, 3, 9));
        assert_eq!((g.world_faces, g.entity_faces, g.displacement_faces, g.triangles), (300, 20, 4, 900));
        assert_eq!((g.displacements, g.displacement_powers), (4, [1, 2, 1]));
        assert_eq!(g.map_size, Some([2048.0, 1024.0, 512.0]));
        assert_eq!(g.bsp_bytes, Some(123_456));
        assert_eq!(
            (g.estimated_luxels, g.estimated_lightmap_bytes, g.max_lightmap_bytes),
            (Some(50_000), Some(14_000_000), 16_777_216)
        );
        assert_eq!(
            g.materials[0],
            NameCount { name: "dev/dev_measurewall01a".into(), count: 200, percent: Some(62.5), ..Default::default() }
        );
        assert_eq!((g.material_total, g.area_total), (7, 2));
        assert_eq!(g.entity_classes[1].name, "func_door");
        assert_eq!(g.brush_entity_classes[0].brushes, Some(6));
        assert_eq!(
            g.areas[0],
            AreaStat { area: 1, leaves: 100, clusters: 40, faces: 280, position: Some([128.0, -64.0, 32.0]) }
        );
        assert_eq!(
            g.small_brushes[1],
            SmallBrush { brush_id: 78, size: [32.0, 8.0, 128.0], position: Some([100.0, 0.0, 64.0]) }
        );
        assert_eq!(g.small_brush_total, 25);
        assert_eq!(g.fadeless_props[0].name, "models/props c17/oildrum001.mdl");
        assert_eq!((g.fadeless_props[0].count, g.fadeless_total), (35, 35));
        assert_eq!(
            g.stages,
            [Timing { name: "read brushes".into(), secs: 0.12 }, Timing { name: "final tree".into(), secs: 1.5 }]
        );

        let v = st.vis.as_ref().unwrap();
        assert_eq!((v.clusters, v.portals, v.leaves, v.mode.as_str()), (120, 340, 800, "full"));
        assert_eq!((v.radius, v.radius_source.as_deref()), (Some(4096.0), Some("env_fog_controller farz")));
        assert_eq!(v.times.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["base", "flow", "pas"]);
        assert_eq!((v.average, v.average_percent, v.max, v.max_percent), (48.0, 40.0, 90, 75.0));
        assert_eq!(
            v.worst[0],
            ClusterStat { cluster: 7, sees: 90, percent: 75.0, faces: 2500, position: Some([10.0, 20.0, 30.0]) }
        );

        let l = st.lighting.as_ref().unwrap();
        assert_eq!((l.faces, l.lit_faces, l.luxels), (320, 300, 70_000));
        assert_eq!(l.passes, [PassBytes { pass: "LDR".into(), bytes: 13_000_000, max_bytes: 16_777_216 }]);
        assert_eq!(l.lights, LightCounts { total: 12, point: 8, spot: 2, sun: 1, sky_ambient: 1, surface: 0 });
        assert_eq!((l.switchable_styles, l.max_switchable_styles, l.styles), (70, 96, 72));
        assert_eq!(
            (l.static_props, l.vertex_lit_props, l.detail_props, l.detail_lit, l.bounces),
            (40, 38, 100, 100, 12)
        );
        assert_eq!((l.ambient_samples, l.ambient_leaves), (900, 450));
        assert_eq!(l.stages.len(), 2);
        assert_eq!(
            l.top_faces[0],
            LuxelFace {
                luxels: 20_000,
                scale: 4.0,
                side_id: 1234,
                size: [512.0, 0.0, 512.0],
                material: "concrete/wall 01".into(),
                position: Some([0.0, 256.0, 128.0])
            }
        );
        assert_eq!(l.top_materials[0].brushes, Some(250));
        assert_eq!(l.top_materials[0].name, "concrete/wall01");
    }

    #[test]
    fn hints_name_places_and_numbers() {
        let st = parse_text(LOG);
        let h = hints(&st);
        let kinds: Vec<&'static str> = h.iter().map(Hint::key).collect();
        for k in [
            "compilestats.hint.near_limit",
            "compilestats.hint.wide_view",
            "compilestats.hint.weak_vis",
            "compilestats.hint.small_brushes",
            "compilestats.hint.fadeless_props",
            "compilestats.hint.power4",
            "compilestats.hint.fine_lightmap",
            "compilestats.hint.switchable_styles",
        ] {
            assert!(kinds.contains(&k), "{k} in {kinds:?}");
        }
        assert!(!kinds.contains(&"compilestats.hint.lightmap_estimate"));
        assert!(!kinds.contains(&"compilestats.hint.lightmap_bytes"), "13 MB of lightmaps load fine");
        assert!(!h.iter().any(|x| matches!(&x.kind, HintKind::NearLimit { name, .. } if name.contains("lightdata"))));
        assert!(h.windows(2).all(|w| w[0].level >= w[1].level));
        let wide = h.iter().find(|x| matches!(x.kind, HintKind::WideView { .. })).unwrap();
        assert_eq!(wide.location.as_ref().and_then(|l| l.position), Some([10.0, 20.0, 30.0]));
        assert_eq!(
            wide.english(),
            "Cluster 7 sees 75% of the map (2500 faces drawn from there): hint brushes, areaportals or turning the open details into func_detail there cut what is drawn."
        );
        let small = h.iter().find(|x| matches!(x.kind, HintKind::SmallBrushes { .. })).unwrap();
        assert_eq!(small.location.as_ref().and_then(|l| l.brush_id), Some(77));
        let fine = h.iter().find(|x| matches!(x.kind, HintKind::FineLightmap { .. })).unwrap();
        assert_eq!(fine.location.as_ref().and_then(|l| l.side_id), Some(1234));
        let limit = h.iter().find(|x| matches!(x.kind, HintKind::NearLimit { .. })).unwrap();
        assert_eq!(limit.level, HintLevel::Important);
        assert!(limit.english().contains("MAX_MAP_BRUSHSIDES"));
        for x in &h {
            assert!(!x.english().is_empty() && !x.english().contains('{'), "{}", x.english());
        }
        let lines = hint_lines(&st);
        assert_eq!(lines[0], HINTS_HEADER);
        for l in &lines {
            assert_eq!(crate::parse_log_line(l), (Severity::Info, None), "{l}");
        }
        assert!(lines.iter().any(|l| l.ends_with("(at 10 20 30)")));
    }

    #[test]
    fn a_reused_visibility_keeps_its_mode_and_is_flagged() {
        let text = |mode: &str| {
            format!(
                "---- Visibility statistics ----\nClusters: 120, portals: 340, leaves: 800 (500 empty)\n\
                 Vis mode: {mode}\nVis radius: none\nVis times: reused 0.045 s, total 0.045 s\n\
                 Visible clusters: average 48.0 (40.0%), max 90 (75.0%) in cluster 7\n"
            )
        };
        for (line, mode, reused) in [
            ("full (reused)", "full", true),
            ("fast (reused)", "fast", true),
            ("full", "full", false),
            ("fast", "fast", false),
            ("off", "off", false),
            ("skipped (leak)", "skipped (leak)", false),
        ] {
            let st = parse_text(&text(line));
            let v = st.vis.as_ref().expect("vis");
            assert_eq!((v.mode.as_str(), v.reused), (mode, reused), "{line}");
            assert_eq!(v.times, [Timing { name: "reused".into(), secs: 0.045 }]);
            let no_vis = hints(&st).iter().any(|h| h.key().starts_with("compilestats.hint.no_vis"));
            assert_eq!(no_vis, mode == "off", "{line}");
        }
        let json = serde_json::to_value(parse_text(&text("full (reused)"))).expect("json");
        assert_eq!((json["vis"]["mode"].as_str(), json["vis"]["reused"].as_bool()), (Some("full"), Some(true)));
    }

    #[test]
    fn only_geometry_gives_the_estimate_and_no_vis() {
        let text = "---- Geometry details ----\nLightmaps (estimate): 1 luxels, 2200000000 bytes per pass of 2147483647 (102.4%)\n\
                    ---- Visibility statistics ----\nClusters: 50, portals: 90, leaves: 200 (100 empty)\nVis mode: off\n";
        let h = hints(&parse_text(text));
        assert_eq!(h[0].key(), "compilestats.hint.lightmap_estimate");
        assert_eq!(h[0].level, HintLevel::Critical);
        assert!(h.iter().any(|x| x.key() == "compilestats.hint.no_vis"));
        assert!(!h.iter().any(|x| x.key() == "compilestats.hint.no_vis_water"), "no water");
        let wet = text.replace(
            "---- Geometry details ----\n",
            "---- Geometry details ----\nBrushes: 40 total, 30 structural, 5 detail, 2 water, 2 tool, 2 in brush entities\n",
        );
        let h = hints(&parse_text(&wet));
        let w = h.iter().find(|x| x.key() == "compilestats.hint.no_vis_water").expect("water hint");
        assert_eq!(w.level, HintLevel::Critical);
        assert!(w.english().starts_with("Water brushes: 2, and no visibility"), "{}", w.english());
        assert!(parse_text("nothing here\n").is_empty());
        let old = "---- Geometry details ----\nLightmaps (estimate): 1 luxels, 20000000 bytes per pass of 16777216 (119.2%)\n";
        assert!(hints(&parse_text(old)).is_empty(), "20 MB of lightmaps are no problem for the game");
        let big = "---- Lighting statistics ----\nLightmap bytes: HDR 81214012 of 2147483647 (3.8%)\n\
                   Lightmap bytes: LDR 2000000000 of 2147483647 (93.1%)\n";
        let h = hints(&parse_text(big));
        assert_eq!(h.len(), 1, "{h:?}");
        assert_eq!((h[0].key(), h[0].level), ("compilestats.hint.lightmap_bytes", HintLevel::Important));
        assert!(h[0].english().starts_with("The LDR lightmaps take 93% of the 2 GB"), "{}", h[0].english());
    }

    #[test]
    fn valve_vbspinfo_rows_are_read() {
        let r = parse_limit_row("brushes                 13/65535         156/786420   ( 0.0%) ").unwrap();
        assert_eq!(
            (r.name.as_str(), r.used, r.max, r.bytes, r.max_bytes, r.percent),
            ("brushes", Some(13), 65_535, 156, 786_420, 0.0)
        );
        let r = parse_limit_row("LDR worldlights      12/8192        1056/720896   ( 0.1%)").unwrap();
        assert_eq!(r.name, "LDR worldlights");
        let r = parse_limit_row("visdata               [variable]    0/16777216     ( 0.0%)").unwrap();
        assert_eq!(r.used, None);
        assert!(parse_limit_row("Clusters: 120, portals: 340").is_none());
        assert!(parse_limit_row("12/13 14/15 (1%)").is_none());
    }
}
