use std::collections::HashMap;

use crate::hub::{Field, Hub, find_cm_vis, find_hub};
use crate::hubdec::{self, Face, MemLeaf, Profile};
use crate::memmap::MemoryCm;
use crate::procmem::MemSource;
use crate::world::{RenderFace, RenderTexInfo, RenderWorld};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fidelity {
    Exact,
    Equivalent,
    Partial,
}

impl Fidelity {
    pub fn id(self) -> &'static str {
        match self {
            Fidelity::Exact => "exact",
            Fidelity::Equivalent => "equivalent",
            Fidelity::Partial => "partial",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LumpPiece {
    pub index: usize,
    pub bytes: Vec<u8>,
    pub version: u16,
    pub fidelity: Fidelity,
    pub note: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MaterialInfo {
    pub reflectivity: [f32; 3],
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HubData {
    pub profile: String,
    pub base: u64,
    pub ptr: usize,
    pub hdr: bool,
    pub pieces: Vec<LumpPiece>,
    pub faces: Vec<Face>,
    pub vertindices: Vec<u16>,
    pub vertexes: Vec<[f32; 3]>,
    pub materials: HashMap<u64, MaterialInfo>,
    pub model_faces: Vec<(i32, i32)>,
    pub lightdata: u64,
    pub missing: Vec<String>,
    pub problems: Vec<String>,
    pub cm_leaf_drop: usize,
    pub texdata: usize,
    pub names: Vec<String>,
    pub disp_extras: HashMap<usize, hubdec::DispExtra>,
    pub alt_contents: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HubLumpRow {
    pub lump: usize,
    pub name: String,
    pub bytes: usize,
    pub fidelity: String,
    pub note: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HubReport {
    pub found: bool,
    pub profile: String,
    pub base: u64,
    pub ptr: usize,
    pub faces: usize,
    pub hdr: bool,
    pub lumps: Vec<HubLumpRow>,
    pub missing: Vec<String>,
    pub problems: Vec<String>,
}

const GROUPS: &[(&str, &[usize])] = &[
    ("visibility, nodes, leaves, models and portals", &[4, 5, 10, 14, 16, 20, 21, 36, 41, 46]),
    ("lighting, lightmaps, ambient light and world lights", &[7, 8, 15, 51, 52, 53, 54, 55, 56, 58]),
    ("physics collision", &[29]),
    ("vertex normals, primitives and occluders", &[9, 30, 31, 37, 38, 39]),
    ("displacement lightmap samples", &[34]),
    ("texture data", &[2, 43, 44]),
];

const NOT_KEPT: &[usize] = &[11, 27, 47, 59, 60];

impl HubReport {
    pub fn source_rows(&self) -> Vec<crate::merge::SourceRow> {
        let mut out = Vec::new();
        if !self.found {
            return out;
        }
        for (name, lumps) in GROUPS {
            let rows: Vec<&HubLumpRow> = self.lumps.iter().filter(|l| lumps.contains(&l.lump)).collect();
            if rows.is_empty() {
                continue;
            }
            let count = |f: &str| rows.iter().filter(|r| r.fidelity == f).count();
            let bytes: usize = rows.iter().map(|r| r.bytes).sum();
            out.push(crate::merge::SourceRow {
                category: (*name).to_owned(),
                source: "memory".to_owned(),
                detail: format!(
                    "{} lumps, {} bytes: {} exact, {} equivalent, {} partial",
                    rows.len(),
                    bytes,
                    count("exact"),
                    count("equivalent"),
                    count("partial")
                ),
            });
        }
        let absent: Vec<&str> = NOT_KEPT.iter().filter(|i| !self.lumps.iter().any(|l| l.lump == **i)).map(|i| hp_bsp::lump_name(*i)).collect();
        if !absent.is_empty() {
            out.push(crate::merge::SourceRow {
                category: "lumps only the compilers and tools read".to_owned(),
                source: "absent".to_owned(),
                detail: absent.join(", "),
            });
        }
        out
    }

    pub fn render(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.found {
            out.push("engine world structure: not recognized in the game memory".to_owned());
            return out;
        }
        let total: usize = self.lumps.iter().map(|l| l.bytes).sum();
        out.push(format!(
            "engine world structure recognized ({} layout, {} lighting): {} lumps rebuilt from memory, {} bytes",
            self.profile,
            if self.hdr { "HDR" } else { "LDR" },
            self.lumps.len(),
            total
        ));
        for l in &self.lumps {
            out.push(format!("  lump {:2} {:<28} {:>10} bytes  {:<10} {}", l.lump, l.name, l.bytes, l.fidelity, l.note));
        }
        for m in &self.missing {
            out.push(format!("  not recognized in this build: {m}"));
        }
        for m in &self.problems {
            out.push(format!("  problem: {m}"));
        }
        out
    }
}

impl HubData {
    pub fn report(&self) -> HubReport {
        let mut lumps: Vec<HubLumpRow> = self
            .pieces
            .iter()
            .map(|p| HubLumpRow {
                lump: p.index,
                name: hp_bsp::lump_name(p.index).to_owned(),
                bytes: p.bytes.len(),
                fidelity: p.fidelity.id().to_owned(),
                note: p.note.clone(),
            })
            .collect();
        lumps.sort_by_key(|l| l.lump);
        HubReport {
            found: true,
            profile: self.profile.clone(),
            base: self.base,
            ptr: self.ptr,
            faces: self.faces.len(),
            hdr: self.hdr,
            lumps,
            missing: self.missing.clone(),
            problems: self.problems.clone(),
        }
    }

    pub fn texdata_count(&self) -> Option<usize> {
        (self.texdata > 0).then_some(self.texdata)
    }

    pub fn piece(&self, index: usize) -> Option<&LumpPiece> {
        self.pieces.iter().find(|p| p.index == index)
    }

    pub fn bytes(&self, index: usize) -> usize {
        self.piece(index).map(|p| p.bytes.len()).unwrap_or(0)
    }

    pub fn total_bytes(&self) -> usize {
        self.pieces.iter().map(|p| p.bytes.len()).sum()
    }

    pub fn push(&mut self, index: usize, bytes: Vec<u8>, version: u16, fidelity: Fidelity, note: &str) {
        self.pieces.retain(|p| p.index != index);
        self.pieces.push(LumpPiece { index, bytes, version, fidelity, note: note.to_owned() });
    }

    pub fn render_world(&self, hub: &Hub, p: &Profile, texinfo: Vec<RenderTexInfo>) -> RenderWorld {
        let faces: Vec<RenderFace> = self
            .faces
            .iter()
            .map(|f| RenderFace {
                plane: f.plane,
                back: f.back,
                first: f.first,
                count: f.count,
                texinfo: f.texinfo,
                disp: f.disp_ptr,
                has_disp: f.flags & crate::world::FACE_HAS_DISP != 0,
                material_sort: 0,
            })
            .collect();
        RenderWorld {
            faces_addr: hub.surfaces2.wrapping_sub(p.s2_flags as u64 + 4),
            faces,
            texinfo_addr: hub.addr(Field::TexInfo),
            texinfo,
            vertindices_addr: hub.addr(Field::VertIndices),
            vertindices: self.vertindices.clone(),
            vertexes_addr: hub.addr(Field::Vertexes),
            vertexes: self.vertexes.clone(),
            ..RenderWorld::default()
        }
    }
}

pub fn read_texinfo<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile) -> Option<Vec<RenderTexInfo>> {
    let raw = hubdec::flat_array(src, hub, Field::TexInfo, p.texinfo_stride)?;
    let mut out = Vec::with_capacity(raw.len() / p.texinfo_stride);
    for r in raw.chunks_exact(p.texinfo_stride) {
        let mut vecs = [0f32; 16];
        for (k, v) in vecs.iter_mut().enumerate() {
            *v = f32::from_le_bytes([r[k * 4], r[k * 4 + 1], r[k * 4 + 2], r[k * 4 + 3]]);
        }
        let flags = i32::from_le_bytes([
            r[p.texinfo_flags],
            r[p.texinfo_flags + 1],
            r[p.texinfo_flags + 2],
            r[p.texinfo_flags + 3],
        ]);
        let material = if p.ptr == 8 {
            u64::from_le_bytes([
                r[p.texinfo_material],
                r[p.texinfo_material + 1],
                r[p.texinfo_material + 2],
                r[p.texinfo_material + 3],
                r[p.texinfo_material + 4],
                r[p.texinfo_material + 5],
                r[p.texinfo_material + 6],
                r[p.texinfo_material + 7],
            ])
        } else {
            u64::from(u32::from_le_bytes([
                r[p.texinfo_material],
                r[p.texinfo_material + 1],
                r[p.texinfo_material + 2],
                r[p.texinfo_material + 3],
            ]))
        };
        out.push(RenderTexInfo { vecs, flags: flags & 0xffff, material });
    }
    Some(out)
}

pub fn read_material_info<S: MemSource + ?Sized>(src: &S, ptr: u64, p: &Profile) -> Option<MaterialInfo> {
    let (refl_at, tex_at, dims_at) = (p.material_reflectivity_at?, p.material_texture_at?, p.texture_dims_at?);
    let base = ptr.wrapping_add_signed(refl_at);
    let r = src.read_vec(base, 12);
    if r.len() != 12 {
        return None;
    }
    let f = |o: usize| f32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
    let reflectivity = [f(0), f(4), f(8)];
    if !reflectivity.iter().all(|v| v.is_finite() && (0.0..=16.0).contains(v)) {
        return None;
    }
    let tex_ptr_raw = src.read_vec(ptr.wrapping_add_signed(tex_at), p.ptr);
    if tex_ptr_raw.len() != p.ptr {
        return None;
    }
    let tex = if p.ptr == 8 {
        u64::from_le_bytes([
            tex_ptr_raw[0],
            tex_ptr_raw[1],
            tex_ptr_raw[2],
            tex_ptr_raw[3],
            tex_ptr_raw[4],
            tex_ptr_raw[5],
            tex_ptr_raw[6],
            tex_ptr_raw[7],
        ])
    } else {
        u64::from(u32::from_le_bytes([tex_ptr_raw[0], tex_ptr_raw[1], tex_ptr_raw[2], tex_ptr_raw[3]]))
    };
    let dims = src.read_vec(tex.wrapping_add(dims_at), 4);
    if tex == 0 || dims.len() != 4 {
        return Some(MaterialInfo { reflectivity, width: 0, height: 0 });
    }
    let (w, h) = (u16::from_le_bytes([dims[0], dims[1]]), u16::from_le_bytes([dims[2], dims[3]]));
    let plausible = |v: u16| v == 0 || (v.is_power_of_two() && v <= 8192);
    if !plausible(w) || !plausible(h) {
        return Some(MaterialInfo { reflectivity, width: 0, height: 0 });
    }
    Some(MaterialInfo { reflectivity, width: w, height: h })
}

pub fn texdata_lump(count: usize, infos: &[Option<MaterialInfo>]) -> Vec<u8> {
    let mut out = Vec::with_capacity(count * 32);
    for i in 0..count {
        let info = infos.get(i).copied().flatten().unwrap_or_default();
        for v in info.reflectivity {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&(i as i32).to_le_bytes());
        let (w, h) = (i32::from(info.width), i32::from(info.height));
        for v in [w, h, w, h] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

pub fn align_cm_leafs(cm_leafs: &[crate::memmap::CLeaf], leafs: &[MemLeaf]) -> Option<usize> {
    if leafs.is_empty() || cm_leafs.len() < leafs.len() {
        return None;
    }
    let same = |a: &crate::memmap::CLeaf, b: &MemLeaf| {
        a.contents == b.contents && a.cluster == b.cluster && a.area_flags == ((b.area as u16 & 0x1ff) | ((b.flags as u16) << 9))
    };
    let slack = cm_leafs.len() - leafs.len();
    (0..=slack.min(4)).find(|d| leafs.iter().enumerate().all(|(i, l)| cm_leafs.get(i + d).is_some_and(|c| same(c, l))))
}

pub fn edge_lumps(vertindices: &[u16], faces: &[Face]) -> (Vec<u8>, Vec<u8>) {
    let n = vertindices.len();
    let mut covered = vec![false; n];
    let mut order: Vec<usize> = (0..faces.len()).collect();
    order.sort_by_key(|&i| faces[i].first);
    let mut edges: Vec<(u16, u16)> = vec![(0, 0)];
    let mut index: HashMap<(u16, u16), i32> = HashMap::new();
    let mut surf = vec![0i32; n];
    for &fi in &order {
        let f = &faces[fi];
        let first = f.first.max(0) as usize;
        let count = usize::from(f.count);
        if first + count > n || count == 0 {
            continue;
        }
        for k in 0..count {
            let a = vertindices[first + k];
            let b = vertindices[first + (k + 1) % count];
            let value = if let Some(&e) = index.get(&(b, a)) {
                -e
            } else if let Some(&e) = index.get(&(a, b)) {
                e
            } else {
                let e = edges.len() as i32;
                edges.push((a, b));
                index.insert((a, b), e);
                e
            };
            surf[first + k] = value;
            covered[first + k] = true;
        }
    }
    for i in 0..n {
        if covered[i] {
            continue;
        }
        let a = vertindices[i];
        let b = if i + 1 < n && !covered[i + 1] { vertindices[i + 1] } else { a };
        let e = edges.len() as i32;
        edges.push((a, b));
        surf[i] = e;
    }
    let mut e_out = Vec::with_capacity(edges.len() * 4);
    for (a, b) in &edges {
        e_out.extend_from_slice(&a.to_le_bytes());
        e_out.extend_from_slice(&b.to_le_bytes());
    }
    let mut s_out = Vec::with_capacity(n * 4);
    for v in surf {
        s_out.extend_from_slice(&v.to_le_bytes());
    }
    (e_out, s_out)
}

fn light_pieces<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile, hdr: bool, d: &mut HubData) {
    let hdr_idx = |ldr: usize, hdr_i: usize| if hdr { hdr_i } else { ldr };
    if let Some(v) = hubdec::worldlight_lump(src, hub, hdr) {
        d.push(
            hdr_idx(15, 54),
            v,
            0,
            Fidelity::Exact,
            "world lights, the radius the engine computes is cleared again",
        );
    }
    if let Some(index) = hubdec::ambient_index_lump(src, hub, p) {
        let n = hubdec::ambient_samples(&index);
        d.push(hdr_idx(52, 51), index, 0, Fidelity::Equivalent, "leaf ambient index, first sample of empty leaves is not kept");
        if let Some(a) = read_raw(src, hub.ambient_lighting, n * 28) {
            d.push(hdr_idx(56, 55), a, 1, Fidelity::Exact, "leaf ambient lighting");
        }
    }

    let faces = hubdec::read_faces(src, hub, p);
    if let Some(faces) = faces {
        let len = hubdec::lighting_len(&faces, hub.lightdata);
        if len > 0 && hub.lightdata != 0 {
            let light = src.read_vec(hub.lightdata, len);
            if light.len() == len {
                d.push(hdr_idx(8, 53), light, 1, Fidelity::Exact, "lightmap samples, length from the face records");
            }
        }
        let lump = hubdec::face_lump(&faces, hub.lightdata, &d.vertindices, &d.vertexes, &HashMap::new());
        d.push(hdr_idx(7, 58), lump.clone(), 1, Fidelity::Equivalent, "faces, area is recomputed, original face is not kept");
        d.push(
            47,
            vec![0xff; faces.len() * 2],
            0,
            Fidelity::Equivalent,
            "face macro texture index: the engine does not keep it, none is assumed (0xffff for every face)",
        );
        d.faces = faces;
    } else {
        d.missing.push("faces".to_owned());
        d.problems.push(format!(
            "the face records ({} bytes each) do not pass the plausibility checks: the face layout of this build is not known",
            p.s2_stride
        ));
    }

}

pub fn capture_lighting<S: MemSource + ?Sized>(src: &S, base: u64, ptr: usize, hdr: bool) -> Option<HubData> {
    let hub = crate::hub::parse_hub(src, base, ptr)?;
    if hub.count(Field::Surfaces) == 0 || hub.count(Field::Planes) == 0 {
        return None;
    }
    let p = hubdec::profiles_for(ptr).into_iter().find(|p| hubdec::read_faces(src, &hub, p).is_some())?;
    let mut d = HubData { profile: p.name.to_owned(), base, ptr, hdr, lightdata: hub.lightdata, ..HubData::default() };
    if let Some(v) = hubdec::flat_array(src, &hub, Field::VertIndices, 2) {
        d.vertindices = v.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    }
    if let Some(v) = hubdec::flat_array(src, &hub, Field::Vertexes, 12) {
        d.vertexes = v
            .chunks_exact(12)
            .map(|c| {
                let f = |o: usize| f32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]]);
                [f(0), f(4), f(8)]
            })
            .collect();
    }
    light_pieces(src, &hub, &p, hdr, &mut d);
    Some(d)
}

fn profile_score<S: MemSource + ?Sized>(src: &S, hub: &Hub, p: &Profile, cm: &MemoryCm) -> usize {
    let mut score = 0;
    if let Some(leafs) = hubdec::read_leafs(src, hub, p)
        && align_cm_leafs(&cm.leafs, &leafs).is_some()
    {
        score += 2;
    }
    if hubdec::node_lump(src, hub, p).is_some() {
        score += 1;
    }
    if hubdec::read_faces(src, hub, p).is_some() {
        score += 1;
    }
    score
}

fn best_profile<S: MemSource + ?Sized>(src: &S, hub: &Hub, cm: &MemoryCm) -> Option<Profile> {
    let mut best: Option<(usize, Profile)> = None;
    for p in hubdec::profiles_for(hub.ptr) {
        let s = profile_score(src, hub, &p, cm);
        if best.as_ref().is_none_or(|b| s > b.0) {
            best = Some((s, p));
        }
    }
    best.map(|b| b.1)
}

pub fn capture<S: MemSource + ?Sized>(src: &S, cm: &MemoryCm, widths: &[usize], hdr: bool) -> Option<(Hub, HubData)> {
    let planes_addr = cm.cm.planes_addr;
    let planes = cm.cm.planes;
    let hub = find_hub(src, planes_addr, planes, widths)?;
    let p = best_profile(src, &hub, cm)?;
    let mut d = HubData { profile: p.name.to_owned(), base: hub.base, ptr: hub.ptr, hdr, lightdata: hub.lightdata, ..HubData::default() };
    for pr in &hub.problems {
        d.problems.push(pr.clone());
    }
    if hub.lightdata == 0 {
        d.problems.push("the engine has not loaded any lightmap data (yet)".to_owned());
    }

    if let Some(v) = hubdec::flat_array(src, &hub, Field::Vertexes, 12) {
        d.vertexes = v
            .chunks_exact(12)
            .map(|c| {
                let f = |o: usize| f32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]]);
                [f(0), f(4), f(8)]
            })
            .collect();
        d.push(3, v, 0, Fidelity::Exact, "vertex array");
    } else {
        d.missing.push("vertexes".to_owned());
    }
    if let Some(v) = hubdec::flat_array(src, &hub, Field::VertIndices, 2) {
        d.vertindices = v.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    } else {
        d.missing.push("vertex indices".to_owned());
    }
    let vis = cm.vis.or_else(|| find_cm_vis(src, planes_addr, planes, hub.ptr).map(|t| (t.vis_addr, t.vis_len)));
    match vis.and_then(|(addr, len)| read_raw(src, addr, len)) {
        Some(v) => d.push(4, v, 0, Fidelity::Exact, "visibility data of the collision world"),
        None => d.missing.push("visibility".to_owned()),
    }
    match hubdec::node_lump(src, &hub, &p) {
        Some(v) => d.push(5, v, 0, Fidelity::Exact, "node array"),
        None => {
            d.missing.push("nodes".to_owned());
            d.problems.push(format!(
                "the node records ({} bytes each) do not pass the plausibility checks: the node layout of this build is not known",
                p.node_stride
            ));
        }
    }
    let mem_leafs = hubdec::read_leafs(src, &hub, &p);
    if let Some(leafs) = &mem_leafs {
        let aligned = align_cm_leafs(&cm.leafs, leafs);
        if aligned.is_none() && !cm.leafs.is_empty() {
            d.problems.push(format!(
                "the leaf records ({} bytes each) do not agree with the collision leaves: the leaf layout of this build is not known",
                p.leaf_stride
            ));
        }
        let drop = aligned.unwrap_or(0);
        d.cm_leaf_drop = drop;
        let brushes: Vec<(u16, u16)> = cm.leafs.iter().skip(drop).map(|l| (l.first_brush, l.num_brushes)).collect();
        d.push(10, hubdec::leaf_lump(leafs, &brushes), 1, Fidelity::Exact, "leaf array plus collision brush ranges");
    } else {
        d.missing.push("leafs".to_owned());
    }
    match hubdec::leaf_faces_lump(src, &hub, &p) {
        Some(v) => d.push(16, v, 0, Fidelity::Exact, "mark surface pointers turned into face indices"),
        None => d.missing.push("leaf faces".to_owned()),
    }
    if let Some(v) = hubdec::leafwater_lump(src, &hub) {
        d.push(36, v, 0, Fidelity::Exact, "leaf water data (padding cleared)");
    }
    if let Some(v) = hubdec::primitive_lump(src, &hub, &p) {
        d.push(37, v, 0, Fidelity::Exact, "primitives");
    }
    if let Some(v) = hubdec::flat_array(src, &hub, Field::PrimVerts, 12) {
        d.push(38, v, 0, Fidelity::Exact, "primitive vertices");
    }
    if let Some(v) = hubdec::flat_array(src, &hub, Field::PrimIndices, 2) {
        d.push(39, v, 0, Fidelity::Exact, "primitive indices");
    }
    if let Some(v) = hubdec::flat_array(src, &hub, Field::Areas, 8) {
        d.push(20, v, 0, Fidelity::Exact, "areas");
    }
    if let Some(v) = hubdec::flat_array(src, &hub, Field::AreaPortals, 12) {
        d.push(21, v, 0, Fidelity::Exact, "area portals");
    }
    if let Some(v) = hubdec::flat_array(src, &hub, Field::ClipPortalVerts, 12) {
        d.push(41, v, 0, Fidelity::Exact, "clip portal vertices");
    }
    if let Some(v) = hubdec::flat_array(src, &hub, Field::VertNormals, 12) {
        d.push(30, v, 0, Fidelity::Exact, "vertex normals");
    }
    if let Some(v) = hubdec::flat_array(src, &hub, Field::VertNormalIndices, 2) {
        d.push(31, v, 0, Fidelity::Exact, "vertex normal indices");
    }
    {
        let (a, b, c) = (hub.array(Field::Occluders), hub.array(Field::OccluderPolys), hub.array(Field::OccluderIndices));
        if a.count >= 0 && b.count >= 0 && c.count >= 0 {
            let parts = (
                read_raw(src, a.addr, a.count as usize * 40),
                read_raw(src, b.addr, b.count as usize * 12),
                read_raw(src, c.addr, c.count as usize * 4),
            );
            if let (Some(oc), Some(po), Some(ix)) = parts {
                let mut v = Vec::with_capacity(12 + oc.len() + po.len() + ix.len());
                v.extend_from_slice(&(a.count as i32).to_le_bytes());
                v.extend_from_slice(&oc);
                v.extend_from_slice(&(b.count as i32).to_le_bytes());
                v.extend_from_slice(&po);
                v.extend_from_slice(&(c.count as i32).to_le_bytes());
                v.extend_from_slice(&ix);
                d.push(9, v, 2, Fidelity::Exact, "occluders, their polygons and vertex indices");
            }
        }
    }
    let n_leafs = hub.count(Field::Leafs);
    if let Some(v) = read_raw(src, hub.addr(Field::LeafMinDist), n_leafs * 2) {
        d.push(46, v, 0, Fidelity::Exact, "leaf minimum distance to water");
    }
    let names = hubdec::texdata_names(src, &hub, &p);
    if let Some(names) = &names {
        d.texdata = names.len();
        d.names = names.iter().map(|n| n.1.clone()).collect();
    }
    if let Some(names) = &names
        && let Some((data, table)) = hubdec::string_lumps(src, names)
    {
        d.push(43, data, 0, Fidelity::Exact, "texture name strings");
        d.push(44, table, 0, Fidelity::Exact, "texture name table");
    }

    light_pieces(src, &hub, &p, hdr, &mut d);

    let texinfo = read_texinfo(src, &hub, &p).unwrap_or_default();
    let mut infos: HashMap<u64, MaterialInfo> = HashMap::new();
    for t in &texinfo {
        if let std::collections::hash_map::Entry::Vacant(e) = infos.entry(t.material)
            && let Some(m) = read_material_info(src, t.material, &p)
        {
            e.insert(m);
        }
    }
    d.materials = infos;

    if !cm.nodes.is_empty() && cm.nodes.len() != hub.count(Field::Nodes) {
        d.problems.push(format!(
            "the world structure has {} nodes and the collision world {}",
            hub.count(Field::Nodes),
            cm.nodes.len()
        ));
    }
    if !cm.leafs.is_empty() && cm.leafs.len() < hub.count(Field::Leafs) {
        d.problems.push(format!(
            "the world structure has {} leaves and the collision world only {}",
            hub.count(Field::Leafs),
            cm.leafs.len()
        ));
    }
    if !cm.models.is_empty() && cm.models.len() != hub.count(Field::Submodels) {
        d.problems.push(format!(
            "the world structure has {} models and the collision world {}",
            hub.count(Field::Submodels),
            cm.models.len()
        ));
    }
    if hub.submodel_objects.len() == hub.count(Field::Submodels) {
        d.model_faces = hub
            .submodel_objects
            .iter()
            .map(|&a| {
                let r = src.read_vec(a + p.model_first as u64, 8);
                if r.len() == 8 {
                    (
                        i32::from_le_bytes([r[0], r[1], r[2], r[3]]),
                        i32::from_le_bytes([r[4], r[5], r[6], r[7]]),
                    )
                } else {
                    (0, 0)
                }
            })
            .collect();
    } else {
        d.missing.push("model face ranges".to_owned());
    }
    if let Some(v) = hubdec::cubemap_lump(src, &hub, &p) {
        d.push(42, v, 0, Fidelity::Exact, "cubemap samples with their sizes");
    }
    let mut disp_faces: Vec<(usize, [f32; 3])> = Vec::new();
    for (i, f) in d.faces.iter().enumerate() {
        if f.disp_ptr != 0
            && let Some(x) = hubdec::read_disp_extra(src, f.disp_ptr, &p)
        {
            let head = read_raw(src, f.disp_ptr + p.disp_start as u64, 12).unwrap_or_default();
            if head.len() == 12 {
                let v = |k: usize| f32::from_le_bytes([head[k], head[k + 1], head[k + 2], head[k + 3]]);
                disp_faces.push((i, [v(0), v(4), v(8)]));
            }
            d.disp_extras.insert(i, x);
        }
    }
    let starts: Vec<[f32; 3]> = disp_faces.iter().map(|x| x.1).collect();
    if let Some(contents) = hubdec::alt_contents(src, &starts) {
        let mut set = 0usize;
        for ((face, _), c) in disp_faces.iter().zip(contents) {
            if let Some(x) = d.disp_extras.get_mut(face)
                && c != 0
            {
                x.contents = c;
                set += 1;
            }
        }
        d.alt_contents = set;
    }
    let last = d
        .disp_extras
        .iter()
        .max_by_key(|(_, x)| x.sample_start)
        .map(|(face, x)| (x.sample_start.max(0) as usize, d.faces.get(*face).map_or(0, hubdec::luxels)));
    if let Some((start, luxels)) = last
        && let Some((addr, len)) = hubdec::find_sample_positions(src, start, luxels, p.ptr)
        && let Some(v) = read_raw(src, addr, len)
    {
        d.push(34, v, 0, Fidelity::Exact, "displacement lightmap sample positions");
    }
    match crate::modsig::map_flags(src, hub.ptr == 8) {
        Some(f) => {
            let (fidelity, note) = if f.inferred_low_bit {
                (Fidelity::Equivalent, "map flags rebuilt from the booleans the engine derives (bit 0 inferred from bit 1)")
            } else {
                (Fidelity::Exact, "map flags rebuilt from the booleans the engine derives")
            };
            d.push(59, f.value.to_le_bytes().to_vec(), 0, fidelity, note);
        }
        None => d.missing.push("map flags".to_owned()),
    }
    let phys = hubdec::read_phys_models(src, cm.models_addr, cm.models.len(), &p);
    if !phys.is_empty() {
        let lost = hubdec::undecoded_keys(&phys);
        let (fidelity, note) = if lost == 0 {
            (Fidelity::Exact, "collision solids from the physics objects, key values decoded from the packed form".to_owned())
        } else {
            (
                Fidelity::Partial,
                format!("collision solids from the physics objects, the key values of {lost} models could not be decoded and are minimal"),
            )
        };
        d.push(29, hubdec::physcollide_lump(&phys), 0, fidelity, &note);
    } else {
        d.missing.push("physics collision".to_owned());
    }
    Some((hub, d))
}

fn read_raw<S: MemSource + ?Sized>(src: &S, addr: u64, len: usize) -> Option<Vec<u8>> {
    if len == 0 {
        return Some(Vec::new());
    }
    if addr == 0 {
        return None;
    }
    let v = src.read_vec(addr, len);
    (v.len() == len).then_some(v)
}

#[cfg(test)]
#[path = "hubdata_tests.rs"]
mod tests;
