use glam::DVec3;
use hp_kv::Block;

mod format;
pub mod import_map;
pub mod manifest;
pub mod multiblend;
mod num;
mod read;
mod report;
mod save;
mod scan;
mod side_flags;
pub mod timings;
mod write;

pub use format::{LineEnding, NumberStyle, TextEncoding, TextFormat};
pub use hp_kv::ParseIssues;
pub use manifest::{Manifest, ManifestEntry, ManifestError};
pub use report::LoadReport;
pub use save::{SaveOptions, SaveReport, save_with};
pub use timings::LoadTimings;

pub type Id = u32;

pub fn is_world_particle_volume(class: &str) -> bool {
    ["func_smokevolume", "env_embers", "func_dustcloud", "func_dustmotes"]
        .iter()
        .any(|name| class.trim().eq_ignore_ascii_case(name))
}

pub const MAX_DISP_POWER: u32 = 4;
pub const CONTENTS_DETAIL: u32 = 0x0800_0000;

pub const MAX_NESTING: usize = 256;

#[derive(Debug, thiserror::Error)]
pub enum VmfError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("not a VMF text file: the file holds binary data")]
    NotText,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map {
    pub version_info: VersionInfo,
    pub visgroups: Vec<VisGroup>,
    pub view_settings: Block,
    pub world: Entity,
    pub entities: Vec<Entity>,
    pub cameras: Cameras,
    pub cordons: Cordons,
    pub extra_blocks: Vec<Block>,
    pub format: TextFormat,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VersionInfo {
    pub editor_version: i32,
    pub editor_build: i32,
    pub map_version: i32,
    pub format_version: i32,
    pub prefab: bool,
    pub extra_props: Vec<(String, String)>,
}

impl Default for VersionInfo {
    fn default() -> Self {
        Self {
            editor_version: 400,
            editor_build: 9540,
            map_version: 1,
            format_version: 100,
            prefab: false,
            extra_props: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct VisGroup {
    pub name: String,
    pub id: Id,
    pub color: [u8; 3],
    pub children: Vec<VisGroup>,
    pub extra_props: Vec<(String, String)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entity {
    pub id: Id,
    pub classname: String,
    pub props: Vec<(String, String)>,
    pub connections: Vec<Connection>,
    pub solids: Vec<Solid>,
    pub groups: Vec<Group>,
    pub editor: EditorInfo,
    pub hidden: bool,
    pub extra_blocks: Vec<Block>,
}

impl Entity {
    pub fn get(&self, key: &str) -> Option<&str> {
        self.props.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        if let Some(slot) = self.props.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            slot.1 = value;
        } else {
            self.props.push((key.to_owned(), value));
        }
    }
    pub fn remove(&mut self, key: &str) -> Option<String> {
        let idx = self.props.iter().position(|(k, _)| k.eq_ignore_ascii_case(key))?;
        Some(self.props.remove(idx).1)
    }
    pub fn origin(&self) -> Option<DVec3> {
        self.get("origin").and_then(parse_vec3)
    }
    pub fn angles(&self) -> Option<DVec3> {
        self.get("angles").and_then(parse_vec3)
    }
    pub fn is_brush_entity(&self) -> bool {
        !self.solids.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Connection {
    pub output: String,
    pub target: String,
    pub input: String,
    pub param: String,
    pub delay: f32,
    pub times: i32,
    pub separator: char,
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            output: String::new(),
            target: String::new(),
            input: String::new(),
            param: String::new(),
            delay: 0.0,
            times: -1,
            separator: ',',
        }
    }
}

pub const CONNECTION_ESC: char = '\x1b';

impl Connection {
    pub fn parse(output: &str, value: &str) -> Connection {
        read::parse_connection(output.to_owned(), value)
    }

    pub fn effective_separator(&self) -> char {
        let needs_esc = self.target.contains(',') || self.input.contains(',') || self.param.contains(',');
        if self.separator == CONNECTION_ESC || needs_esc { CONNECTION_ESC } else { ',' }
    }

    pub fn to_value_string(&self) -> String {
        let mut s = String::with_capacity(self.target.len() + self.input.len() + self.param.len() + 16);
        write::push_connection_value(&mut s, self);
        s
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub id: Id,
    pub editor: EditorInfo,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorInfo {
    pub color: [u8; 3],
    pub visgroup_ids: Vec<Id>,
    pub group_id: Option<Id>,
    pub visgroup_shown: bool,
    pub visgroup_auto_shown: bool,
    pub logical_pos: Option<String>,
    pub comments: Option<String>,
    pub extra_props: Vec<(String, String)>,
}

impl Default for EditorInfo {
    fn default() -> Self {
        Self {
            color: [0, 180, 255],
            visgroup_ids: Vec::new(),
            group_id: None,
            visgroup_shown: true,
            visgroup_auto_shown: true,
            logical_pos: None,
            comments: None,
            extra_props: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Solid {
    pub id: Id,
    pub sides: Vec<Side>,
    pub editor: EditorInfo,
    pub hidden: bool,
    pub extra_props: Vec<(String, String)>,
    pub extra_blocks: Vec<Block>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TexAxis {
    pub axis: DVec3,
    pub shift: f64,
    pub scale: f64,
}

impl Default for TexAxis {
    fn default() -> Self {
        Self { axis: DVec3::X, shift: 0.0, scale: 0.25 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Side {
    pub id: Id,
    pub plane: [DVec3; 3],
    pub material: String,
    pub uaxis: TexAxis,
    pub vaxis: TexAxis,
    pub rotation: f64,
    pub lightmap_scale: i32,
    pub smoothing_groups: u32,
    pub disp: Option<Box<DispInfo>>,
    pub vertices_plus: Option<Vec<DVec3>>,
    pub extra_props: Vec<(String, String)>,
    pub extra_blocks: Vec<Block>,
}

impl Default for Side {
    fn default() -> Self {
        Self {
            id: 0,
            plane: [DVec3::ZERO, DVec3::X, DVec3::Y],
            material: "TOOLS/TOOLSNODRAW".to_owned(),
            uaxis: TexAxis { axis: DVec3::X, shift: 0.0, scale: 0.25 },
            vaxis: TexAxis { axis: DVec3::NEG_Y, shift: 0.0, scale: 0.25 },
            rotation: 0.0,
            lightmap_scale: 16,
            smoothing_groups: 0,
            disp: None,
            vertices_plus: None,
            extra_props: Vec::new(),
            extra_blocks: Vec::new(),
        }
    }
}

impl Side {
    pub const UNREADABLE_PLANE: [DVec3; 3] = [DVec3::ZERO; 3];

    pub fn normal(&self) -> DVec3 {
        let [a, b, c] = self.plane;
        (a - b).cross(c - b).normalize_or_zero()
    }

    pub fn plane_distance(&self) -> f64 {
        self.normal().dot(self.plane[0])
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DispInfo {
    pub power: u32,
    pub start_position: DVec3,
    pub flags: i32,
    pub elevation: f64,
    pub subdiv: bool,
    pub normals: Vec<DVec3>,
    pub distances: Vec<f64>,
    pub offsets: Vec<DVec3>,
    pub offset_normals: Vec<DVec3>,
    pub alphas: Vec<f64>,
    pub triangle_tags: Vec<i32>,
    pub allowed_verts: Vec<i32>,
    pub extra_props: Vec<(String, String)>,
    pub extra_blocks: Vec<Block>,
}

pub const DISP_CAST_SHADOWS_KEY: &str = "hp_cast_shadows";

impl DispInfo {
    pub fn row_len(&self) -> usize {
        1usize.checked_shl(self.power).unwrap_or(0).saturating_add(1)
    }

    pub fn casts_shadows(&self) -> bool {
        !self.extra_props.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case(DISP_CAST_SHADOWS_KEY) && v.trim().parse::<f64>().is_ok_and(|x| x == 0.0)
        })
    }

    pub fn set_casts_shadows(&mut self, on: bool) -> bool {
        if self.casts_shadows() == on {
            return false;
        }
        self.extra_props.retain(|(k, _)| !k.eq_ignore_ascii_case(DISP_CAST_SHADOWS_KEY));
        if !on {
            self.extra_props.push((DISP_CAST_SHADOWS_KEY.to_owned(), "0".to_owned()));
        }
        true
    }
    pub fn new_flat(power: u32, start: DVec3, normal: DVec3) -> Self {
        let power = power.min(MAX_DISP_POWER);
        let n = (1usize << power) + 1;
        let count = n * n;
        let tris = 2 * (n - 1) * (n - 1);
        Self {
            power,
            start_position: start,
            flags: 0,
            elevation: 0.0,
            subdiv: false,
            normals: vec![normal; count],
            distances: vec![0.0; count],
            offsets: vec![DVec3::ZERO; count],
            offset_normals: vec![normal; count],
            alphas: vec![0.0; count],
            triangle_tags: vec![9; tris],
            allowed_verts: vec![-1; 10],
            extra_props: Vec::new(),
            extra_blocks: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Camera {
    pub position: DVec3,
    pub look: DVec3,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cameras {
    pub active: i32,
    pub cameras: Vec<Camera>,
}

impl Default for Cameras {
    fn default() -> Self {
        Self { active: -1, cameras: Vec::new() }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cordon {
    pub name: String,
    pub active: bool,
    pub mins: DVec3,
    pub maxs: DVec3,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CordonStyle {
    #[default]
    Legacy,
    Modern,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CordonBox {
    pub cordon: usize,
    pub mins: DVec3,
    pub maxs: DVec3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cordons {
    pub active: bool,
    pub cordons: Vec<Cordon>,
    pub style: CordonStyle,
    pub extra_boxes: Vec<CordonBox>,
}

pub fn parse_vec3(s: &str) -> Option<DVec3> {
    let s = s.trim().trim_start_matches(['(', '[']).trim_end_matches([')', ']']);
    let mut it = s.split_ascii_whitespace().map(|p| p.parse::<f64>());
    let x = it.next()?.ok()?;
    let y = it.next()?.ok()?;
    let z = it.next()?.ok()?;
    Some(DVec3::new(x, y, z))
}

pub fn fmt_vec3(v: DVec3) -> String {
    let mut s = String::new();
    hp_kv::fmt_num(&mut s, v.x);
    s.push(' ');
    hp_kv::fmt_num(&mut s, v.y);
    s.push(' ');
    hp_kv::fmt_num(&mut s, v.z);
    s
}

impl Map {
    pub fn new_empty() -> Self {
        let mut view_settings = Block::new("viewsettings");
        for (k, v) in [
            ("bSnapToGrid", "1"),
            ("bShowGrid", "1"),
            ("bShowLogicalGrid", "0"),
            ("nGridSpacing", "64"),
            ("bShow3DGrid", "0"),
        ] {
            view_settings.push_prop(k, v);
        }
        let world = Entity {
            id: 1,
            classname: "worldspawn".to_owned(),
            props: [
                ("detailmaterial", "detail/detailsprites"),
                ("detailvbsp", "detail.vbsp"),
                ("maxpropscreenwidth", "-1"),
                ("skyname", "sky_day01_01"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect(),
            ..Entity::default()
        };
        Self {
            version_info: VersionInfo::default(),
            visgroups: Vec::new(),
            view_settings,
            world,
            entities: Vec::new(),
            cameras: Cameras::default(),
            cordons: Cordons {
                active: false,
                cordons: vec![Cordon {
                    name: "cordon".to_owned(),
                    active: true,
                    mins: DVec3::splat(-1024.0),
                    maxs: DVec3::splat(1024.0),
                }],
                style: CordonStyle::Legacy,
                extra_boxes: Vec::new(),
            },
            extra_blocks: Vec::new(),
            format: TextFormat::default(),
        }
    }

    pub fn max_object_id(&self) -> Id {
        let ents = std::iter::once(&self.world).chain(self.entities.iter());
        ents.map(|e| {
            let solids = e.solids.iter().map(|s| s.id).max().unwrap_or(0);
            let groups = e.groups.iter().map(|g| g.id).max().unwrap_or(0);
            e.id.max(solids).max(groups)
        })
        .max()
        .unwrap_or(0)
    }

    pub fn max_side_id(&self) -> Id {
        self.all_solids().flat_map(|s| s.sides.iter()).map(|s| s.id).max().unwrap_or(0)
    }

    pub fn all_solids(&self) -> impl Iterator<Item = &Solid> + '_ {
        self.world.solids.iter().chain(self.entities.iter().flat_map(|e| e.solids.iter()))
    }

    pub fn all_solids_mut(&mut self) -> impl Iterator<Item = &mut Solid> + '_ {
        self.world.solids.iter_mut().chain(self.entities.iter_mut().flat_map(|e| e.solids.iter_mut()))
    }

    pub fn entity_by_id(&self, id: Id) -> Option<&Entity> {
        self.entities.iter().find(|e| e.id == id)
    }

    pub fn entity_by_id_mut(&mut self, id: Id) -> Option<&mut Entity> {
        self.entities.iter_mut().find(|e| e.id == id)
    }

    pub fn solid_by_id(&self, id: Id) -> Option<&Solid> {
        self.all_solids().find(|s| s.id == id)
    }
}

pub fn parse(src: &str) -> Result<Map, VmfError> {
    parse_with_report(src).map(|(map, _)| map)
}

pub fn parse_with_report(src: &str) -> Result<(Map, LoadReport), VmfError> {
    let format = TextFormat::detect(src.as_bytes());
    parse_text(src, format)
}

fn parse_text(src: &str, format: TextFormat) -> Result<(Map, LoadReport), VmfError> {
    if scan::looks_binary(src.as_bytes()) {
        return Err(VmfError::NotText);
    }
    let start = std::time::Instant::now();
    let (tops, syntax) = scan::scan(src)?;
    let scanned = std::time::Instant::now();
    let mut map = read::build_map(tops)?;
    map.format = format;
    let report = LoadReport::new(syntax, &map);
    timings::record(|t| {
        t.scan = scanned - start;
        t.build = scanned.elapsed();
    });
    Ok((map, report))
}

pub fn parse_bytes(bytes: &[u8]) -> Result<Map, VmfError> {
    parse_bytes_with_report(bytes).map(|(map, _)| map)
}

pub fn parse_bytes_with_report(bytes: &[u8]) -> Result<(Map, LoadReport), VmfError> {
    if scan::looks_binary(bytes) {
        return Err(VmfError::NotText);
    }
    let start = std::time::Instant::now();
    let (text, encoding) = hp_kv::decode(bytes);
    let format = TextFormat { encoding, ..TextFormat::detect(bytes) };
    let decode = start.elapsed();
    let result = parse_text(&text, format);
    timings::record(|t| t.decode = decode);
    result
}

pub fn from_kv(root: Block) -> Result<Map, VmfError> {
    read::build_map(read::tops_from_root(root))
}

pub fn load(path: &std::path::Path) -> Result<Map, VmfError> {
    load_with_report(path).map(|(map, _)| map)
}

pub fn load_with_report(path: &std::path::Path) -> Result<(Map, LoadReport), VmfError> {
    timings::reset();
    let start = std::time::Instant::now();
    let bytes = timings::read_file(path)?;
    let read = start.elapsed();
    timings::record(|t| {
        t.bytes = bytes.len() as u64;
        t.read = read;
    });
    parse_bytes_with_report(&bytes)
}

pub fn last_load_timings() -> LoadTimings {
    timings::last()
}

pub fn write(map: &Map) -> String {
    write::write_map(map)
}

pub fn write_bytes(map: &Map) -> (Vec<u8>, TextEncoding) {
    let text = write(map);
    let (bytes, used) = hp_kv::encode(&text, map.format.encoding);
    let bytes = match bytes {
        std::borrow::Cow::Borrowed(_) => text.into_bytes(),
        std::borrow::Cow::Owned(b) => b,
    };
    (bytes, used)
}

pub fn save(map: &Map, path: &std::path::Path) -> Result<(), VmfError> {
    save_with(map, path, &SaveOptions::default()).map(|_| ())
}

pub fn write_solid(solid: &Solid) -> String {
    write::write_solid_fragment(solid)
}

pub fn write_entity(entity: &Entity) -> String {
    write::write_entity_fragment(entity)
}

pub fn parse_fragment(src: &str) -> Result<(Vec<Solid>, Vec<Entity>), VmfError> {
    parse_fragment_full(src).map(|f| (f.solids, f.entities))
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fragment {
    pub solids: Vec<Solid>,
    pub entities: Vec<Entity>,
    pub groups: Vec<Group>,
    pub visgroups: Vec<VisGroup>,
}

pub fn parse_fragment_full(src: &str) -> Result<Fragment, VmfError> {
    let (tops, _) = scan::scan(src)?;
    read::build_fragment(tops)
}
