use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

pub const DATACACHE_MODULE: &str = "datacache.dll";
pub const MDLCACHE_VTABLE_RVA: u64 = 0x49768;
const CACHE_DICT_AT: u64 = 0x40;
const CACHE_COUNT_AT: u64 = 0x48;
const ELEMENT_SIZE: usize = 24;
const ELEMENT_NAME_AT: usize = 8;
const ELEMENT_DATA_AT: usize = 16;
const MAX_ELEMENTS: usize = 1 << 20;
const MODULE_REACH: u64 = 0x40_0000;
pub const DATA_VCOLLIDE_AT: u64 = 8;
pub const DATA_HARDWARE_AT: u64 = 0x28;
const HW_ROOT_LOD_AT: usize = 0;
const HW_NUM_LODS_AT: usize = 4;
const HW_LODS_AT: usize = 8;
const HW_NUM_MESHES_AT: usize = 16;
const HW_SIZE: usize = 20;
const LOD_RECORD: u64 = 0x30;
const LOD_MESHES_AT: usize = 0;
const LOD_SWITCH_AT: usize = 8;
const LOD_MATERIALS_COUNT_AT: usize = 12;
const LOD_MATERIALS_AT: usize = 16;
const MESH_RECORD: u64 = 16;
const GROUP_RECORD: u64 = 0x48;
const GROUP_STRIPS_AT: usize = 8;
const GROUP_FLAGS_AT: usize = 12;
const GROUP_STRIP_DATA_AT: usize = 16;
const GROUP_MAP_AT: usize = 24;
const GROUP_VERTICES_AT: usize = 32;
const GROUP_INDICES_AT: usize = 48;
const MAX_LODS: usize = 16;
const MAX_MESHES: usize = 4096;
const MAX_GROUPS: usize = 4096;
const MAX_STRIPS: usize = 4096;
const MAX_VERTICES: usize = 1 << 17;
const MAX_INDICES: usize = 1 << 22;

pub const VTX_VERSION: i32 = 7;
pub const VTX_HEADER: usize = 36;
pub const VTX_BODYPART: usize = 8;
pub const VTX_MODEL: usize = 8;
pub const VTX_LOD: usize = 12;
pub const VTX_MESH: usize = 9;
pub const VTX_GROUP: usize = 25;
pub const VTX_STRIP: usize = 27;
pub const VTX_VERTEX: usize = 9;
pub const VTX_BONE_CHANGE: usize = 8;
pub const VTX_REPLACEMENT_LIST: usize = 8;
pub const VTX_REPLACEMENT: usize = 6;
const VTX_CACHE_SIZE: i32 = 24;
const VTX_MAX_BONES_STRIP: u16 = 53;
const VTX_MAX_BONES_TRI: u16 = 9;
const VTX_MAX_BONES_VERT: i32 = 3;

const MDL_HEADER2_INDEX: usize = 400;
const MODERN_HEADER2_MAX: i32 = 600;
const MDL_BODYPART_COUNT: usize = 232;
const MDL_BODYPART_INDEX: usize = 236;
const BODYPART_RECORD: usize = 16;
const MODEL_RECORD: usize = 148;
const MODEL_MESH_COUNT: usize = 72;
const MODEL_MESH_INDEX: usize = 76;
const MODEL_VERTEX_INDEX: usize = 84;
const MESH_STUDIO_RECORD: usize = 116;
const MESH_VERTICES: usize = 8;
const MESH_VERTEX_OFFSET: usize = 12;
const MESH_MATERIAL_TYPE: usize = 24;
const MESH_ID: usize = 32;
const VVD_VERTEX: usize = 48;
const VVD_HEADER_VERTEX_START: usize = 56;

fn user_pointer(p: u64) -> bool {
    (0x1_0000..0x7fff_ffff_ffff).contains(&p)
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn u64_at(d: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StudioEntry {
    pub name: String,
    pub data: u64,
}

fn dictionary<S: MemSource + ?Sized>(src: &S, cache: u64) -> Option<(u64, usize)> {
    let head = src.read_vec(cache + CACHE_DICT_AT, 0x10);
    if head.len() != 0x10 {
        return None;
    }
    let dict = u64_at(&head, 0);
    let count = usize::try_from(i32_at(&head, (CACHE_COUNT_AT - CACHE_DICT_AT) as usize)).ok()?;
    if !user_pointer(dict) || count == 0 || count > MAX_ELEMENTS {
        return None;
    }
    let first = src.read_vec(dict, ELEMENT_SIZE);
    (first.len() == ELEMENT_SIZE).then_some((dict, count))
}

pub fn find_cache<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Option<u64> {
    let base = src.module_base(DATACACHE_MODULE)?;
    let needle = (base + MDLCACHE_VTABLE_RVA).to_le_bytes();
    let finder = memchr::memmem::Finder::new(&needle).into_owned();
    let mut hits: Vec<u64> = Vec::new();
    for r in src.regions() {
        if !r.writable || r.base + r.size <= base || r.base >= base + MODULE_REACH {
            continue;
        }
        let mut off = 0u64;
        while off < r.size {
            let n = (r.size - off).min(8 << 20) as usize;
            let data = src.read_vec(r.base + off, n);
            hits.extend(
                finder
                    .find_iter(&data)
                    .filter(|&j| (r.base + off + j as u64).is_multiple_of(8))
                    .map(|j| r.base + off + j as u64),
            );
            off += n as u64;
        }
    }
    if hits.is_empty() {
        let scan = ScanOptions { overlap: 8, writable_only: true, ..*opts };
        hits = par_chunks(src, &scan, |b, data, own| {
            finder
                .find_iter(data)
                .filter(|&j| j < own && (b + j as u64).is_multiple_of(8))
                .map(|j| b + j as u64)
                .collect::<Vec<u64>>()
        })
        .into_iter()
        .flatten()
        .collect();
    }
    hits.sort_unstable();
    hits.into_iter().find(|&a| dictionary(src, a).is_some())
}

fn read_text<S: MemSource + ?Sized>(src: &S, addr: u64) -> Option<String> {
    let raw = src.read_vec(addr, 260);
    let end = raw.iter().position(|b| *b == 0)?;
    if end == 0 {
        return None;
    }
    let text = std::str::from_utf8(&raw[..end]).ok()?;
    text.bytes().all(|b| (0x20..0x7f).contains(&b)).then(|| text.to_owned())
}

pub fn normalize_name(name: &str) -> String {
    let mut s = name.trim().replace(char::from(92u8), "/").to_ascii_lowercase();
    if !s.starts_with("models/") {
        s = format!("models/{s}");
    }
    s
}

pub fn entries<S: MemSource + ?Sized>(src: &S, cache: u64) -> Vec<StudioEntry> {
    let Some((dict, count)) = dictionary(src, cache) else { return Vec::new() };
    let raw = src.read_vec(dict, count * ELEMENT_SIZE);
    let mut out = Vec::new();
    for (i, e) in raw.chunks_exact(ELEMENT_SIZE).enumerate() {
        if usize::from(u16_at(e, 0)) == i {
            continue;
        }
        let (name_at, data) = (u64_at(e, ELEMENT_NAME_AT), u64_at(e, ELEMENT_DATA_AT));
        if !user_pointer(name_at) || !user_pointer(data) {
            continue;
        }
        let Some(name) = read_text(src, name_at) else { continue };
        if name.to_ascii_lowercase().ends_with(".mdl") {
            out.push(StudioEntry { name: normalize_name(&name), data });
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct StripRec {
    pub header: [u8; VTX_STRIP],
    pub changes: Vec<(i32, i32)>,
}

impl StripRec {
    pub fn indices(&self) -> usize {
        i32_at(&self.header, 0).max(0) as usize
    }

    pub fn vertex_offset(&self) -> usize {
        i32_at(&self.header, 12).max(0) as usize
    }

    pub fn vertices(&self) -> usize {
        i32_at(&self.header, 8).max(0) as usize
    }

    pub fn slot_of(&self, bone: i32) -> Option<i32> {
        self.changes.iter().find(|c| c.1 == bone).map(|c| c.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct HwGroup {
    pub flags: i32,
    pub strips: Vec<StripRec>,
    pub group_to_mesh: Vec<u16>,
    pub indices: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HwLod {
    pub switch_point: f32,
    pub meshes: Vec<Vec<HwGroup>>,
    pub materials: Vec<u64>,
    pub names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hardware {
    pub root_lod: i32,
    pub lods: Vec<Option<HwLod>>,
}

fn read_group<S: MemSource + ?Sized>(src: &S, at: u64) -> Option<HwGroup> {
    let g = src.read_vec(at, GROUP_RECORD as usize);
    if g.len() != GROUP_RECORD as usize {
        return None;
    }
    let strips_n = usize::try_from(i32_at(&g, GROUP_STRIPS_AT)).ok()?;
    let verts_n = usize::try_from(i32_at(&g, GROUP_VERTICES_AT)).ok()?;
    if strips_n > MAX_STRIPS || verts_n > MAX_VERTICES {
        return None;
    }
    let strip_data = u64_at(&g, GROUP_STRIP_DATA_AT);
    let map = u64_at(&g, GROUP_MAP_AT);
    let indices_at = u64_at(&g, GROUP_INDICES_AT);
    let mut strips = Vec::with_capacity(strips_n);
    let mut index_count = 0usize;
    if strips_n > 0 {
        if !user_pointer(strip_data) {
            return None;
        }
        for k in 0..strips_n {
            let at = strip_data + (k * VTX_STRIP) as u64;
            let h = src.read_vec(at, VTX_STRIP);
            if h.len() != VTX_STRIP {
                return None;
            }
            let mut header = [0u8; VTX_STRIP];
            header.copy_from_slice(&h);
            let changes_n = usize::try_from(i32_at(&h, 19)).ok()?;
            let changes_off = i32_at(&h, 23);
            if changes_n > 4096 {
                return None;
            }
            let mut changes = Vec::with_capacity(changes_n);
            if changes_n > 0 {
                let raw = src.read_vec(at.checked_add_signed(i64::from(changes_off))?, changes_n * VTX_BONE_CHANGE);
                if raw.len() != changes_n * VTX_BONE_CHANGE {
                    return None;
                }
                for c in raw.chunks_exact(VTX_BONE_CHANGE) {
                    changes.push((i32_at(c, 0), i32_at(c, 4)));
                }
            }
            index_count += i32_at(&h, 0).max(0) as usize;
            strips.push(StripRec { header, changes });
        }
    }
    if index_count > MAX_INDICES {
        return None;
    }
    let group_to_mesh = if verts_n == 0 {
        Vec::new()
    } else {
        let raw = src.read_vec(map, verts_n * 2);
        if raw.len() != verts_n * 2 {
            return None;
        }
        raw.chunks_exact(2).map(|c| u16_at(c, 0)).collect()
    };
    let indices = if index_count == 0 {
        Vec::new()
    } else {
        let raw = src.read_vec(indices_at, index_count * 2);
        if raw.len() != index_count * 2 {
            return None;
        }
        raw.chunks_exact(2).map(|c| u16_at(c, 0)).collect()
    };
    Some(HwGroup { flags: i32_at(&g, GROUP_FLAGS_AT), strips, group_to_mesh, indices })
}

const MATERIAL_MODULE: &str = "materialsystem.dll";
const MATERIAL_NAMES_CELL_RVA: u64 = 0x0211_93d8;
const MATERIAL_NAME_AT: usize = 0x28;

pub fn material_name<S: MemSource + ?Sized>(src: &S, material: u64) -> Option<String> {
    if !user_pointer(material) {
        return None;
    }
    let base = src.module_base(MATERIAL_MODULE)?;
    let table = crate::matmem::read_u64(src, base + MATERIAL_NAMES_CELL_RVA);
    let rec = src.read_vec(material + MATERIAL_NAME_AT as u64, 2);
    if rec.len() != 2 {
        return None;
    }
    crate::matmem::read_symbol(src, table, u16_at(&rec, 0)).filter(|n| !n.is_empty())
}

pub fn read_hardware<S: MemSource + ?Sized>(src: &S, data: u64) -> Option<Hardware> {
    let hw = src.read_vec(data + DATA_HARDWARE_AT, HW_SIZE);
    if hw.len() != HW_SIZE {
        return None;
    }
    let root_lod = i32_at(&hw, HW_ROOT_LOD_AT);
    let num_lods = usize::try_from(i32_at(&hw, HW_NUM_LODS_AT)).ok()?;
    let lods_at = u64_at(&hw, HW_LODS_AT);
    let num_meshes = usize::try_from(i32_at(&hw, HW_NUM_MESHES_AT)).ok()?;
    if num_lods == 0
        || num_lods > MAX_LODS
        || num_meshes > MAX_MESHES
        || !user_pointer(lods_at)
        || !(0..=MAX_LODS as i32).contains(&root_lod)
    {
        return None;
    }
    let mut lods = Vec::with_capacity(num_lods);
    for l in 0..num_lods {
        let rec = src.read_vec(lods_at + l as u64 * LOD_RECORD, LOD_RECORD as usize);
        if rec.len() != LOD_RECORD as usize {
            return None;
        }
        let meshes_at = u64_at(&rec, LOD_MESHES_AT);
        if !user_pointer(meshes_at) {
            lods.push(None);
            continue;
        }
        let switch_point = f32_at(&rec, LOD_SWITCH_AT);
        let material_count = usize::try_from(i32_at(&rec, LOD_MATERIALS_COUNT_AT)).unwrap_or(0).min(1024);
        let materials_at = u64_at(&rec, LOD_MATERIALS_AT);
        let materials: Vec<u64> = if material_count > 0 && user_pointer(materials_at) {
            src.read_vec(materials_at, material_count * 8).chunks_exact(8).map(|c| u64_at(c, 0)).collect()
        } else {
            Vec::new()
        };
        let mut meshes = Vec::with_capacity(num_meshes);
        for k in 0..num_meshes {
            let m = src.read_vec(meshes_at + k as u64 * MESH_RECORD, MESH_RECORD as usize);
            if m.len() != MESH_RECORD as usize {
                return None;
            }
            let group_count = usize::try_from(i32_at(&m, 0)).ok()?;
            let groups_at = u64_at(&m, 8);
            if group_count > MAX_GROUPS {
                return None;
            }
            let mut groups = Vec::with_capacity(group_count);
            if group_count > 0 && !user_pointer(groups_at) {
                return None;
            }
            for g in 0..group_count {
                groups.push(read_group(src, groups_at + g as u64 * GROUP_RECORD)?);
            }
            meshes.push(groups);
        }
        let names = materials.iter().map(|m| material_name(src, *m).unwrap_or_default()).collect();
        lods.push(Some(HwLod { switch_point, meshes, materials, names }));
    }
    Some(Hardware { root_lod, lods })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdlMesh {
    pub id: usize,
    pub vertices: usize,
    pub vertex_offset: usize,
    pub kind: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdlModel {
    pub vertex_base: usize,
    pub meshes: Vec<MdlMesh>,
}

pub fn parse_mdl_bodies(mdl: &[u8]) -> Option<Vec<Vec<MdlModel>>> {
    if mdl.len() < MDL_BODYPART_INDEX + 4 || &mdl[0..4] != b"IDST" {
        return None;
    }
    let parts = usize::try_from(i32_at(mdl, MDL_BODYPART_COUNT)).ok()?;
    let first = usize::try_from(i32_at(mdl, MDL_BODYPART_INDEX)).ok()?;
    if parts > 256 {
        return None;
    }
    let mut out = Vec::with_capacity(parts);
    for p in 0..parts {
        let at = first.checked_add(p * BODYPART_RECORD)?;
        let head = mdl.get(at..at + BODYPART_RECORD)?;
        let models = usize::try_from(i32_at(head, 4)).ok()?;
        let index = usize::try_from(i32_at(head, 12)).ok()?;
        if models > 256 {
            return None;
        }
        let mut list = Vec::with_capacity(models);
        for m in 0..models {
            let mat = at.checked_add(index)?.checked_add(m * MODEL_RECORD)?;
            let rec = mdl.get(mat..mat + MODEL_RECORD)?;
            let mesh_count = usize::try_from(i32_at(rec, MODEL_MESH_COUNT)).ok()?;
            let mesh_index = usize::try_from(i32_at(rec, MODEL_MESH_INDEX)).ok()?;
            let vertex_index = usize::try_from(i32_at(rec, MODEL_VERTEX_INDEX)).unwrap_or(0);
            if mesh_count > MAX_MESHES {
                return None;
            }
            let mut meshes = Vec::with_capacity(mesh_count);
            for k in 0..mesh_count {
                let at_mesh = mat.checked_add(mesh_index)?.checked_add(k * MESH_STUDIO_RECORD)?;
                let mr = mdl.get(at_mesh..at_mesh + MESH_STUDIO_RECORD)?;
                meshes.push(MdlMesh {
                    id: usize::try_from(i32_at(mr, MESH_ID)).ok()?,
                    vertices: usize::try_from(i32_at(mr, MESH_VERTICES)).unwrap_or(0),
                    vertex_offset: usize::try_from(i32_at(mr, MESH_VERTEX_OFFSET)).unwrap_or(0),
                    kind: i32_at(mr, MESH_MATERIAL_TYPE),
                });
            }
            list.push(MdlModel { vertex_base: vertex_index / VVD_VERTEX, meshes });
        }
        out.push(list);
    }
    Some(out)
}

pub fn cd_textures(mdl: &[u8]) -> Vec<String> {
    let (Some(count), Some(index)) = (mdl.get(212..216), mdl.get(216..220)) else { return Vec::new() };
    let count = usize::try_from(i32_at(count, 0)).unwrap_or(0).min(64);
    let index = usize::try_from(i32_at(index, 0)).unwrap_or(0);
    let mut out = Vec::new();
    for k in 0..count {
        let Some(off) = mdl.get(index + k * 4..index + k * 4 + 4) else { break };
        let Ok(at) = usize::try_from(i32_at(off, 0)) else { continue };
        let Some(rest) = mdl.get(at..) else { continue };
        let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len());
        let mut dir = String::from_utf8_lossy(&rest[..end]).replace(char::from(92u8), "/").to_ascii_lowercase();
        if !dir.is_empty() && !dir.ends_with('/') {
            dir.push('/');
        }
        out.push(dir);
    }
    out
}

pub fn short_material_name(full: &str, dirs: &[String]) -> String {
    let name = full.replace(char::from(92u8), "/").to_ascii_lowercase();
    dirs.iter()
        .filter(|d| !d.is_empty() && name.starts_with(d.as_str()))
        .max_by_key(|d| d.len())
        .map_or(name.clone(), |d| name[d.len()..].to_owned())
}

pub fn material_replacements(mdl: &[u8], hw: &Hardware) -> Vec<Vec<(i16, String)>> {
    let dirs = cd_textures(mdl);
    let base = hw.lods.iter().flatten().next();
    hw.lods
        .iter()
        .enumerate()
        .map(|(l, lod)| {
            let (Some(lod), Some(base)) = (lod, base) else { return Vec::new() };
            if l == 0 || std::ptr::eq(lod, base) {
                return Vec::new();
            }
            (0..lod.materials.len().min(base.materials.len()))
                .rev()
                .filter(|&i| {
                    lod.materials[i] != base.materials[i] && !lod.names[i].is_empty() && lod.names[i] != "error"
                })
                .map(|i| (i as i16, short_material_name(&lod.names[i], &dirs)))
                .collect()
        })
        .collect()
}

pub fn consistent(mdl: &[u8], hw: &Hardware) -> bool {
    let Some(bodies) = parse_mdl_bodies(mdl) else { return false };
    let mut any = false;
    for model in bodies.iter().flatten() {
        for mesh in &model.meshes {
            for lod in hw.lods.iter().flatten() {
                let Some(groups) = lod.meshes.get(mesh.id) else { return false };
                for g in groups {
                    any = true;
                    if g.group_to_mesh.iter().any(|v| usize::from(*v) >= mesh.vertices.max(1)) {
                        return false;
                    }
                }
            }
        }
    }
    any
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct VtxStats {
    pub missing_lods: usize,
    pub groups: usize,
    pub unresolved_bones: usize,
    pub unknown_vertices: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RebuiltVtx {
    pub bytes: Vec<u8>,
    pub stats: VtxStats,
}

struct VvdView<'a> {
    bytes: &'a [u8],
    vertex_start: usize,
    count: usize,
}

impl<'a> VvdView<'a> {
    fn new(bytes: &'a [u8]) -> Option<Self> {
        if bytes.len() < 64 || &bytes[0..4] != b"IDSV" {
            return None;
        }
        let vertex_start = usize::try_from(i32_at(bytes, VVD_HEADER_VERTEX_START)).ok()?;
        let tangent_start = usize::try_from(i32_at(bytes, VVD_HEADER_VERTEX_START + 4)).ok()?;
        if tangent_start < vertex_start || tangent_start > bytes.len() {
            return None;
        }
        Some(Self { bytes, vertex_start, count: (tangent_start - vertex_start) / VVD_VERTEX })
    }

    fn vertex(&self, id: usize) -> Option<(usize, [i32; 3])> {
        if id >= self.count {
            return None;
        }
        let at = self.vertex_start + id * VVD_VERTEX;
        let v = self.bytes.get(at..at + VVD_VERTEX)?;
        Some((usize::from(v[15]), [i32::from(v[12] as i8), i32::from(v[13] as i8), i32::from(v[14] as i8)]))
    }
}

fn put_i32(out: &mut [u8], at: usize, v: i32) {
    out[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

struct GroupPlan<'a> {
    group: &'a HwGroup,
    base: usize,
}

pub fn rebuild_vtx(mdl: &[u8], hw: &Hardware, vvd: Option<&[u8]>) -> Option<RebuiltVtx> {
    let bodies = parse_mdl_bodies(mdl)?;
    let checksum = i32_at(mdl, 8);
    let group_size = VTX_GROUP;
    let modern = mdl
        .get(MDL_HEADER2_INDEX..MDL_HEADER2_INDEX + 4)
        .is_some_and(|b| (1..=MODERN_HEADER2_MAX).contains(&i32_at(b, 0)));
    let view = vvd.and_then(VvdView::new);
    let num_lods = hw.lods.len();
    let mut stats = VtxStats::default();
    let empty: Vec<HwGroup> = Vec::new();

    let models_total: usize = bodies.iter().map(Vec::len).sum();
    let lod_headers = models_total * num_lods;
    let meshes_total: usize = bodies.iter().flatten().map(|m| m.meshes.len() * num_lods).sum();

    let mut plans: Vec<GroupPlan> = Vec::new();
    let mut mesh_group_counts: Vec<usize> = Vec::new();
    for model in bodies.iter().flatten() {
        for l in 0..num_lods {
            if hw.lods[l].is_none() {
                stats.missing_lods += 1;
            }
            for mesh in &model.meshes {
                let groups = hw.lods[l].as_ref().and_then(|lod| lod.meshes.get(mesh.id)).unwrap_or(&empty);
                mesh_group_counts.push(groups.len());
                for g in groups {
                    plans.push(GroupPlan { group: g, base: model.vertex_base + mesh.vertex_offset });
                }
            }
        }
    }
    stats.groups = plans.len();
    let strips_total: usize = plans.iter().map(|p| p.group.strips.len()).sum();
    let verts_total: usize = plans.iter().map(|p| p.group.group_to_mesh.len()).sum();
    let indices_total: usize = plans.iter().map(|p| p.group.indices.len()).sum();
    let changes_total: usize = plans
        .iter()
        .flat_map(|p| p.group.strips.iter())
        .map(|s| {
            if s.changes.is_empty() && i16::from_le_bytes([s.header[16], s.header[17]]) == 0 {
                1
            } else {
                s.changes.len()
            }
        })
        .sum();

    let bodypart_at = VTX_HEADER;
    let model_at = bodypart_at + bodies.len() * VTX_BODYPART;
    let lod_at = model_at + models_total * VTX_MODEL;
    let mesh_at = lod_at + lod_headers * VTX_LOD;
    let group_at = mesh_at + meshes_total * VTX_MESH;
    let strip_at = group_at + plans.len() * group_size;
    let vertex_at = strip_at + strips_total * VTX_STRIP;
    let index_at = vertex_at + verts_total * VTX_VERTEX;
    let change_at = index_at + indices_total * 2;
    let replacements = material_replacements(mdl, hw);
    let mut pool: Vec<&str> = Vec::new();
    for (_, name) in replacements.iter().flatten() {
        if !pool.contains(&name.as_str()) {
            pool.push(name);
        }
    }
    let pool_bytes: usize = pool.iter().map(|n| n.len() + 1).sum();
    let pool_at = change_at + changes_total * VTX_BONE_CHANGE;
    let headers_at = pool_at + pool_bytes;
    let header_count: usize = replacements.iter().map(Vec::len).sum();
    let replace_at = headers_at + header_count * VTX_REPLACEMENT;
    let end = replace_at + num_lods * VTX_REPLACEMENT_LIST;
    let mut out = vec![0u8; end];

    put_i32(&mut out, 0, VTX_VERSION);
    put_i32(&mut out, 4, VTX_CACHE_SIZE);
    out[8..10].copy_from_slice(&VTX_MAX_BONES_STRIP.to_le_bytes());
    out[10..12].copy_from_slice(&VTX_MAX_BONES_TRI.to_le_bytes());
    put_i32(&mut out, 12, VTX_MAX_BONES_VERT);
    put_i32(&mut out, 16, checksum);
    put_i32(&mut out, 20, i32::try_from(num_lods).ok()?);
    put_i32(&mut out, 24, i32::try_from(replace_at).ok()?);
    put_i32(&mut out, 28, i32::try_from(bodies.len()).ok()?);
    put_i32(&mut out, 32, i32::try_from(bodypart_at).ok()?);

    let rel = |target: usize, from: usize| i32::try_from(target as i64 - from as i64).ok();
    let (mut model_i, mut lod_i, mut mesh_i, mut group_i, mut strip_i, mut vert_i, mut index_i, mut change_i) =
        (0, 0, 0, 0, 0, 0, 0, 0);
    let mut mesh_counts = mesh_group_counts.iter();
    for (b, models) in bodies.iter().enumerate() {
        let at = bodypart_at + b * VTX_BODYPART;
        put_i32(&mut out, at, i32::try_from(models.len()).ok()?);
        put_i32(&mut out, at + 4, rel(model_at + model_i * VTX_MODEL, at)?);
        for model in models {
            let mat = model_at + model_i * VTX_MODEL;
            model_i += 1;
            put_i32(&mut out, mat, i32::try_from(num_lods).ok()?);
            put_i32(&mut out, mat + 4, rel(lod_at + lod_i * VTX_LOD, mat)?);
            for l in 0..num_lods {
                let lat = lod_at + lod_i * VTX_LOD;
                lod_i += 1;
                put_i32(&mut out, lat, i32::try_from(model.meshes.len()).ok()?);
                put_i32(&mut out, lat + 4, rel(mesh_at + mesh_i * VTX_MESH, lat)?);
                let switch = hw.lods[l].as_ref().map_or(-1.0, |x| x.switch_point);
                out[lat + 8..lat + 12].copy_from_slice(&switch.to_le_bytes());
                for _ in &model.meshes {
                    let mat_mesh = mesh_at + mesh_i * VTX_MESH;
                    mesh_i += 1;
                    let count = *mesh_counts.next()?;
                    put_i32(&mut out, mat_mesh, i32::try_from(count).ok()?);
                    put_i32(&mut out, mat_mesh + 4, rel(group_at + group_i * group_size, mat_mesh)?);
                    for _ in 0..count {
                        let plan = &plans[group_i];
                        let gat = group_at + group_i * group_size;
                        group_i += 1;
                        let g = plan.group;
                        put_i32(&mut out, gat, i32::try_from(g.group_to_mesh.len()).ok()?);
                        put_i32(&mut out, gat + 4, rel(vertex_at + vert_i * VTX_VERTEX, gat)?);
                        put_i32(&mut out, gat + 8, i32::try_from(g.indices.len()).ok()?);
                        put_i32(&mut out, gat + 12, rel(index_at + index_i * 2, gat)?);
                        put_i32(&mut out, gat + 16, i32::try_from(g.strips.len()).ok()?);
                        put_i32(&mut out, gat + 20, rel(strip_at + strip_i * VTX_STRIP, gat)?);
                        out[gat + 24] = g.flags as u8;
                        for (j, id) in g.group_to_mesh.iter().enumerate() {
                            let at = vertex_at + (vert_i + j) * VTX_VERTEX;
                            let strip = g
                                .strips
                                .iter()
                                .find(|s| j >= s.vertex_offset() && j < s.vertex_offset() + s.vertices());
                            let (nb, bones) = view
                                .as_ref()
                                .and_then(|v| v.vertex(plan.base + usize::from(*id)))
                                .unwrap_or_else(|| {
                                    stats.unknown_vertices += 1;
                                    (1, [0, 0, 0])
                                });
                            out[at..at + 3].copy_from_slice(&[0, 1, 2]);
                            let skinned = strip.is_some_and(|s| i16::from_le_bytes([s.header[16], s.header[17]]) != 0);
                            let nb = if skinned { nb.min(3) } else { 0 };
                            out[at + 3] = nb as u8;
                            out[at + 4..at + 6].copy_from_slice(&id.to_le_bytes());
                            let direct = strip.is_some_and(|s| s.changes.is_empty());
                            for k in 0..3 {
                                out[at + 6 + k] = if k >= nb {
                                    if (direct && skinned) || modern { 0xff } else { 0 }
                                } else if direct {
                                    bones[k] as u8
                                } else {
                                    match strip.and_then(|s| s.slot_of(bones[k])) {
                                        Some(slot) => slot as u8,
                                        None => {
                                            stats.unresolved_bones += 1;
                                            0
                                        }
                                    }
                                };
                            }
                        }
                        vert_i += g.group_to_mesh.len();
                        for (j, ix) in g.indices.iter().enumerate() {
                            let at = index_at + (index_i + j) * 2;
                            out[at..at + 2].copy_from_slice(&ix.to_le_bytes());
                        }
                        index_i += g.indices.len();
                        for s in &g.strips {
                            let sat = strip_at + strip_i * VTX_STRIP;
                            strip_i += 1;
                            out[sat..sat + VTX_STRIP].copy_from_slice(&s.header);
                            put_i32(&mut out, sat + 23, rel(change_at + change_i * VTX_BONE_CHANGE, sat)?);
                            for c in &s.changes {
                                let cat = change_at + change_i * VTX_BONE_CHANGE;
                                change_i += 1;
                                put_i32(&mut out, cat, c.0);
                                put_i32(&mut out, cat + 4, c.1);
                            }
                            if s.changes.is_empty() && i16::from_le_bytes([s.header[16], s.header[17]]) == 0 {
                                change_i += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    let mut cursor = pool_at;
    let mut offsets: Vec<(&str, usize)> = Vec::new();
    for n in &pool {
        offsets.push((n, cursor));
        out[cursor..cursor + n.len()].copy_from_slice(n.as_bytes());
        cursor += n.len() + 1;
    }
    let mut header_at = headers_at;
    for (l, list) in replacements.iter().enumerate() {
        let at = replace_at + l * VTX_REPLACEMENT_LIST;
        put_i32(&mut out, at, i32::try_from(list.len()).ok()?);
        put_i32(&mut out, at + 4, rel(header_at, at)?);
        for (id, name) in list {
            out[header_at..header_at + 2].copy_from_slice(&id.to_le_bytes());
            let target = offsets.iter().find(|(n, _)| *n == name.as_str()).map(|(_, o)| *o)?;
            put_i32(&mut out, header_at + 2, rel(target, header_at)?);
            header_at += VTX_REPLACEMENT;
        }
    }
    Some(RebuiltVtx { bytes: out, stats })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    const DATA: u64 = 0x2000_0000;
    const LODS: u64 = 0x2100_0000;
    const MESHES0: u64 = 0x2200_0000;
    const MESHES1: u64 = 0x2210_0000;
    const GROUPS0: u64 = 0x2300_0000;
    const GROUPS1: u64 = 0x2310_0000;
    const STRIPS0: u64 = 0x2400_0000;
    const STRIPS1: u64 = 0x2410_0000;
    const MAP0: u64 = 0x2500_0000;
    const MAP1: u64 = 0x2510_0000;
    const INDICES0: u64 = 0x2600_0000;
    const INDICES1: u64 = 0x2610_0000;

    fn put_i32(v: &mut [u8], at: usize, x: i32) {
        v[at..at + 4].copy_from_slice(&x.to_le_bytes());
    }

    fn put_q(v: &mut [u8], at: usize, x: u64) {
        v[at..at + 8].copy_from_slice(&x.to_le_bytes());
    }

    fn strip(indices: i32, vertices: i32, bones: i16, changes: &[(i32, i32)], offset: i32) -> Vec<u8> {
        let mut s = vec![0u8; VTX_STRIP];
        put_i32(&mut s, 0, indices);
        put_i32(&mut s, 8, vertices);
        s[16..18].copy_from_slice(&bones.to_le_bytes());
        s[18] = 1;
        put_i32(&mut s, 19, i32::try_from(changes.len()).unwrap());
        put_i32(&mut s, 23, offset);
        for c in changes {
            s.extend_from_slice(&c.0.to_le_bytes());
            s.extend_from_slice(&c.1.to_le_bytes());
        }
        s
    }

    fn group(strips: u64, strip_count: i32, flags: i32, map: u64, vertices: i32, indices: u64) -> Vec<u8> {
        let mut g = vec![0u8; GROUP_RECORD as usize];
        put_i32(&mut g, GROUP_STRIPS_AT, strip_count);
        put_i32(&mut g, GROUP_FLAGS_AT, flags);
        put_q(&mut g, GROUP_STRIP_DATA_AT, strips);
        put_q(&mut g, GROUP_MAP_AT, map);
        put_i32(&mut g, GROUP_VERTICES_AT, vertices);
        put_q(&mut g, GROUP_INDICES_AT, indices);
        g
    }

    fn u16s(v: &[u16]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    fn lod(meshes: u64, switch: f32) -> Vec<u8> {
        let mut l = vec![0u8; LOD_RECORD as usize];
        put_q(&mut l, LOD_MESHES_AT, meshes);
        l[LOD_SWITCH_AT..LOD_SWITCH_AT + 4].copy_from_slice(&switch.to_le_bytes());
        l
    }

    fn mesh_record(groups: u64, count: i32) -> Vec<u8> {
        let mut m = vec![0u8; MESH_RECORD as usize];
        put_i32(&mut m, 0, count);
        put_q(&mut m, 8, groups);
        m
    }

    fn memory() -> VecMem {
        memory_with(false, strip(3, 3, 0, &[], 27))
    }

    fn memory_with(unloaded: bool, strips1: Vec<u8>) -> VecMem {
        let mut m = VecMem::default();
        let mut data = vec![0u8; 0x80];
        put_i32(&mut data, DATA_HARDWARE_AT as usize + HW_ROOT_LOD_AT, 0);
        put_i32(&mut data, DATA_HARDWARE_AT as usize + HW_NUM_LODS_AT, 2);
        put_q(&mut data, DATA_HARDWARE_AT as usize + HW_LODS_AT, LODS);
        put_i32(&mut data, DATA_HARDWARE_AT as usize + HW_NUM_MESHES_AT, 2);
        m.add(DATA, data);
        m.add(LODS, [lod(if unloaded { 0 } else { MESHES0 }, 0.0), lod(MESHES1, 35.0)].concat());
        m.add(MESHES0, [mesh_record(GROUPS0, 1), mesh_record(0, 0)].concat());
        m.add(MESHES1, [mesh_record(GROUPS1, 1), mesh_record(0, 0)].concat());
        m.add(GROUPS0, group(STRIPS0, 1, 2, MAP0, 4, INDICES0));
        m.add(GROUPS1, group(STRIPS1, 1, 2, MAP1, 3, INDICES1));
        m.add(STRIPS0, strip(6, 4, 1, &[(7, 5)], 27));
        m.add(STRIPS1, strips1);
        m.add(MAP0, u16s(&[3, 2, 1, 0]));
        m.add(MAP1, u16s(&[2, 1, 0]));
        m.add(INDICES0, u16s(&[0, 1, 2, 2, 1, 3]));
        m.add(INDICES1, u16s(&[0, 1, 2]));
        m
    }

    fn mdl(version: i32) -> Vec<u8> {
        let mut d = vec![0u8; 800];
        d[0..4].copy_from_slice(b"IDST");
        put_i32(&mut d, 4, version);
        put_i32(&mut d, 8, 0x1234);
        put_i32(&mut d, MDL_BODYPART_COUNT, 1);
        put_i32(&mut d, MDL_BODYPART_INDEX, 300);
        put_i32(&mut d, 304, 1);
        put_i32(&mut d, 312, 16);
        let model = 316;
        put_i32(&mut d, model + MODEL_MESH_COUNT, 2);
        put_i32(&mut d, model + MODEL_MESH_INDEX, 148);
        put_i32(&mut d, model + MODEL_VERTEX_INDEX, 0);
        for k in 0..2 {
            let at = model + 148 + k * MESH_STUDIO_RECORD;
            put_i32(&mut d, at + MESH_ID, k as i32);
            put_i32(&mut d, at + MESH_VERTICES, 4);
            put_i32(&mut d, at + MESH_VERTEX_OFFSET, 0);
        }
        d
    }

    fn vvd() -> Vec<u8> {
        let mut d = vec![0u8; 64];
        d[0..4].copy_from_slice(b"IDSV");
        put_i32(&mut d, 4, 4);
        put_i32(&mut d, 8, 0x1234);
        put_i32(&mut d, 56, 64);
        put_i32(&mut d, 60, 64 + 4 * VVD_VERTEX as i32);
        for k in 0..4 {
            let mut v = vec![0u8; VVD_VERTEX];
            v[0..4].copy_from_slice(&1.0f32.to_le_bytes());
            v[12] = 5;
            v[15] = 1;
            v[16 + k] = 1;
            d.extend_from_slice(&v);
        }
        d.extend_from_slice(&[0u8; 4 * 16]);
        d
    }

    #[test]
    fn the_hardware_data_of_a_model_is_read_group_by_group() {
        let m = memory();
        let hw = read_hardware(&m, DATA).expect("hardware");
        assert_eq!((hw.root_lod, hw.lods.len()), (0, 2));
        let lod0 = hw.lods[0].as_ref().unwrap();
        assert_eq!(lod0.switch_point, 0.0);
        let g = &lod0.meshes[0][0];
        assert_eq!(g.flags, 2);
        assert_eq!(g.group_to_mesh, vec![3, 2, 1, 0]);
        assert_eq!(g.indices, vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(g.strips[0].changes, vec![(7, 5)]);
        assert!(lod0.meshes[1].is_empty());
        assert_eq!(hw.lods[1].as_ref().unwrap().switch_point, 35.0);
    }

    #[test]
    fn an_unloaded_lod_is_kept_as_empty() {
        let m = memory_with(true, strip(3, 3, 0, &[], 27));
        let hw = read_hardware(&m, DATA).expect("hardware");
        assert!(hw.lods[0].is_none());
        let built = rebuild_vtx(&mdl(48), &hw, None).expect("vtx");
        assert_eq!(built.stats.missing_lods, 1);
    }

    #[test]
    fn a_strip_file_is_laid_out_like_the_compiler_writes_it() {
        let m = memory();
        let hw = read_hardware(&m, DATA).expect("hardware");
        let built = rebuild_vtx(&mdl(48), &hw, Some(&vvd())).expect("vtx");
        let b = &built.bytes;
        assert_eq!(i32_at(b, 0), VTX_VERSION);
        assert_eq!(i32_at(b, 16), 0x1234);
        assert_eq!((i32_at(b, 20), i32_at(b, 28), i32_at(b, 32)), (2, 1, 36));
        let lod_headers = 36 + VTX_BODYPART + VTX_MODEL;
        assert_eq!(f32_at(b, lod_headers + VTX_LOD + 8), 35.0);
        let mesh_at = lod_headers + 2 * VTX_LOD;
        assert_eq!(i32_at(b, mesh_at), 1);
        let group_at = mesh_at + 4 * VTX_MESH;
        let sg0 = group_at;
        assert_eq!((i32_at(b, sg0), i32_at(b, sg0 + 8), i32_at(b, sg0 + 16), b[sg0 + 24]), (4, 6, 1, 2));
        let verts = sg0 + i32_at(b, sg0 + 4) as usize;
        assert_eq!(&b[verts..verts + 9], &[0, 1, 2, 1, 3, 0, 7, 0, 0]);
        let ids: Vec<u16> = (0..4).map(|k| u16::from_le_bytes([b[verts + k * 9 + 4], b[verts + k * 9 + 5]])).collect();
        assert_eq!(ids, vec![3, 2, 1, 0]);
        let index_at = sg0 + i32_at(b, sg0 + 12) as usize;
        let idx: Vec<u16> =
            (0..6).map(|k| u16::from_le_bytes([b[index_at + k * 2], b[index_at + k * 2 + 1]])).collect();
        assert_eq!(idx, vec![0, 1, 2, 2, 1, 3]);
        let strip_at = sg0 + i32_at(b, sg0 + 20) as usize;
        let changes = strip_at + i32_at(b, strip_at + 23) as usize;
        assert_eq!((i32_at(b, changes), i32_at(b, changes + 4)), (7, 5));
        let replace = i32_at(b, 24) as usize;
        assert_eq!(b.len(), replace + 2 * VTX_REPLACEMENT_LIST);
        assert_eq!(i32_at(b, replace), 0);
    }

    #[test]
    fn a_strip_without_bones_has_zero_bone_vertices_and_a_reserved_state_change() {
        let m = memory();
        let hw = read_hardware(&m, DATA).expect("hardware");
        let built = rebuild_vtx(&mdl(48), &hw, Some(&vvd())).expect("vtx");
        let b = &built.bytes;
        let group_at = 36 + VTX_BODYPART + VTX_MODEL + 2 * VTX_LOD + 4 * VTX_MESH;
        let sg1 = group_at + VTX_GROUP;
        let verts = sg1 + i32_at(b, sg1 + 4) as usize;
        assert_eq!(&b[verts..verts + 9], &[0, 1, 2, 0, 2, 0, 0, 0, 0]);
        let strip_at = sg1 + i32_at(b, sg1 + 20) as usize;
        let first_change = strip_at + i32_at(b, strip_at + 23) as usize;
        assert!(first_change + VTX_BONE_CHANGE <= b.len());
    }

    #[test]
    fn a_strip_with_bones_and_no_state_changes_uses_the_model_bone_numbers() {
        let m = memory_with(false, strip(3, 3, 1, &[], 27));
        let hw = read_hardware(&m, DATA).expect("hardware");
        let built = rebuild_vtx(&mdl(48), &hw, Some(&vvd())).expect("vtx");
        let b = &built.bytes;
        let group_at = 36 + VTX_BODYPART + VTX_MODEL + 2 * VTX_LOD + 4 * VTX_MESH;
        let sg1 = group_at + VTX_GROUP;
        let verts = sg1 + i32_at(b, sg1 + 4) as usize;
        assert_eq!(&b[verts..verts + 9], &[0, 1, 2, 1, 2, 0, 5, 0xff, 0xff]);
    }

    #[test]
    fn a_model_is_consistent_with_its_hardware_data_only_when_the_vertex_ids_fit() {
        let m = memory();
        let hw = read_hardware(&m, DATA).expect("hardware");
        assert!(consistent(&mdl(48), &hw));
        let mut small = mdl(48);
        put_i32(&mut small, 316 + 148 + MESH_VERTICES, 2);
        assert!(!consistent(&small, &hw));
        assert!(!consistent(&[0u8; 20], &hw));
    }

    #[test]
    fn material_replacements_are_the_materials_that_differ_from_the_first_lod() {
        let mut mdl = mdl(48);
        let text_at = 700usize;
        mdl[text_at..text_at + 18].copy_from_slice(b"models/humans/m/\0\0");
        put_i32(&mut mdl, 212, 1);
        put_i32(&mut mdl, 216, 650);
        put_i32(&mut mdl, 650, text_at as i32);
        let lod_a = HwLod {
            switch_point: 0.0,
            meshes: Vec::new(),
            materials: vec![1, 2, 3],
            names: vec!["a".into(), "models/humans/m/b".into(), "c".into()],
        };
        let lod_b = HwLod {
            switch_point: 30.0,
            meshes: Vec::new(),
            materials: vec![1, 9, 8],
            names: vec!["a".into(), "models/humans/m/facemap".into(), "error".into()],
        };
        let hw = Hardware { root_lod: 0, lods: vec![Some(lod_a), Some(lod_b)] };
        assert_eq!(cd_textures(&mdl), vec!["models/humans/m/".to_owned()]);
        assert_eq!(material_replacements(&mdl, &hw), vec![Vec::new(), vec![(1i16, "facemap".to_owned())]]);
        assert_eq!(short_material_name("Models\\Humans\\M\\x", &cd_textures(&mdl)), "x");
        assert_eq!(short_material_name("other/y", &cd_textures(&mdl)), "other/y");
    }

    #[test]
    fn the_cache_dictionary_lists_the_models_by_name() {
        let mut m = VecMem::default();
        m.modules.push((DATACACHE_MODULE.to_owned(), 0x3000_0000));
        let cache = 0x3000_0000u64 + 0x1000;
        let mut obj = vec![0u8; 0x60];
        put_q(&mut obj, 0, 0x3000_0000 + MDLCACHE_VTABLE_RVA);
        put_q(&mut obj, CACHE_DICT_AT as usize, 0x3100_0000);
        put_i32(&mut obj, CACHE_COUNT_AT as usize, 3);
        m.add(cache, obj);
        let mut dict = vec![0u8; 3 * ELEMENT_SIZE];
        dict[0..2].copy_from_slice(&0u16.to_le_bytes());
        dict[ELEMENT_SIZE..ELEMENT_SIZE + 2].copy_from_slice(&7u16.to_le_bytes());
        put_q(&mut dict, ELEMENT_SIZE + ELEMENT_NAME_AT, 0x3200_0000);
        put_q(&mut dict, ELEMENT_SIZE + ELEMENT_DATA_AT, 0x3300_0000);
        dict[2 * ELEMENT_SIZE..2 * ELEMENT_SIZE + 2].copy_from_slice(&9u16.to_le_bytes());
        put_q(&mut dict, 2 * ELEMENT_SIZE + ELEMENT_NAME_AT, 0x3200_0100);
        put_q(&mut dict, 2 * ELEMENT_SIZE + ELEMENT_DATA_AT, 0x3300_1000);
        m.add(0x3100_0000, dict);
        m.add(0x3200_0000, b"models\\Props_C17\\Oil.MDL\0".to_vec());
        m.add(0x3200_0100, b"materials/readme.txt\0".to_vec());
        let found = entries(&m, cache);
        assert_eq!(found, vec![StudioEntry { name: "models/props_c17/oil.mdl".to_owned(), data: 0x3300_0000 }]);
        assert_eq!(find_cache(&m, &ScanOptions::default()), Some(cache));
    }
}
