use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

pub const MATERIAL_MODULE: &str = "materialsystem.dll";
pub const KEYVALUES_MODULE: &str = "vstdlib.dll";
pub const MATERIAL_VTABLE_RVA: u64 = 0xb4f30;
pub const MATERIAL_NAMES_CELL_RVA: u64 = 0x0211_93d8;
pub const KEYVALUES_SYSTEM_VTABLE_RVA: u64 = 0x43ca8;
pub const TEXTURE_VTABLE_RVA: u64 = 0xbd258;
const SHADER_MODULES: [&str; 4] =
    ["stdshader_dx9.dll", "game_shader_generic_garrysmod.dll", "stdshader_dbg.dll", "stdshader_dx8.dll"];
const KEYVALUES_MODULE_REACH: u64 = 0x40_0000;

const MATERIAL_NAME_AT: usize = 0x28;
const MATERIAL_KV_AT: usize = 0xb8;
const MATERIAL_SIZE: usize = 0xc0;
const TABLE_ELEMENTS_AT: u64 = 8;
const TABLE_POOLS_AT: u64 = 0x38;
const ELEMENT_SIZE: u64 = 12;
const KV_SYSTEM_STRINGS_AT: u64 = 0x38;
const KV_NAME_HIGH_AT: usize = 3;
const KV_VALUE_AT: usize = 8;
const KV_WIDE_AT: usize = 0x10;
const KV_NUMBER_AT: usize = 0x18;
const KV_TYPE_AT: usize = 0x20;
const KV_NAME_LOW_AT: usize = 0x22;
const KV_PEER_AT: usize = 0x28;
const KV_SUB_AT: usize = 0x30;
const KV_SIZE: usize = 0x40;
const TEXTURE_NAME_AT: usize = 0x14;
const TEXTURE_FLAGS_AT: usize = 0x18;
const TEXTURE_FORMAT_AT: usize = 0x24;
const TEXTURE_WIDTH_AT: usize = 0x28;
const TEXTURE_HEIGHT_AT: usize = 0x2a;
const TEXTURE_FRAMES_AT: usize = 0x2c;
const TEXTURE_SIZE: usize = 0x40;
const TEXTURE_NO_MIP: u32 = 0x100;
const TEXTURE_CUBE: u32 = 0x4000;
const MAX_NODES: usize = 8192;
const MAX_DEPTH: usize = 24;
const MAX_STRING: usize = 4096;

pub fn read_u64<S: MemSource + ?Sized>(src: &S, addr: u64) -> u64 {
    let b = src.read_vec(addr, 8);
    if b.len() == 8 { u64_at(&b, 0) } else { 0 }
}

fn u64_at(d: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn user_pointer(p: u64) -> bool {
    (0x1_0000..0x7fff_ffff_ffff).contains(&p)
}

pub fn read_cstring<S: MemSource + ?Sized>(src: &S, addr: u64, max: usize) -> Option<String> {
    let mut out = Vec::new();
    let mut at = addr;
    while out.len() < max {
        let chunk = src.read_vec(at, 64.min(max - out.len()));
        if chunk.is_empty() {
            return None;
        }
        if let Some(end) = chunk.iter().position(|b| *b == 0) {
            out.extend_from_slice(&chunk[..end]);
            return Some(String::from_utf8_lossy(&out).into_owned());
        }
        out.extend_from_slice(&chunk);
        at += chunk.len() as u64;
    }
    None
}

pub fn read_symbol<S: MemSource + ?Sized>(src: &S, table: u64, symbol: u16) -> Option<String> {
    if symbol == u16::MAX || !user_pointer(table) {
        return None;
    }
    let elements = read_u64(src, table + TABLE_ELEMENTS_AT);
    let pools = read_u64(src, table + TABLE_POOLS_AT);
    if !user_pointer(elements) || !user_pointer(pools) {
        return None;
    }
    let e = src.read_vec(elements + u64::from(symbol) * ELEMENT_SIZE, ELEMENT_SIZE as usize);
    if e.len() != ELEMENT_SIZE as usize {
        return None;
    }
    let (pool, offset) = (u64::from(u16_at(&e, 8)), u64::from(u16_at(&e, 10)));
    let base = read_u64(src, pools + pool * 8);
    if !user_pointer(base) {
        return None;
    }
    read_cstring(src, base + offset + 10, 256)
}

#[derive(Clone, Debug, PartialEq)]
pub enum KvValue {
    Group,
    Text(String),
    Int(i32),
    Float(f32),
    Color([u8; 4]),
    Pointer(u64),
    Wide(String),
    Big(u64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct KvNode {
    pub name: String,
    pub value: KvValue,
    pub children: Vec<KvNode>,
}

pub struct KeyNames {
    pub strings: u64,
}

impl KeyNames {
    pub fn name<S: MemSource + ?Sized>(&self, src: &S, high: u8, low: u16) -> Option<String> {
        let symbol = u64::from(high) | (u64::from(low) << 8);
        read_cstring(src, self.strings + symbol, 256)
    }
}

pub fn read_key_names<S: MemSource + ?Sized>(src: &S, system: u64) -> Option<KeyNames> {
    let strings = read_u64(src, system + KV_SYSTEM_STRINGS_AT);
    user_pointer(strings).then_some(KeyNames { strings })
}

fn read_wide<S: MemSource + ?Sized>(src: &S, addr: u64) -> Option<String> {
    let raw = src.read_vec(addr, 2048);
    let units: Vec<u16> =
        raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect();
    Some(String::from_utf16_lossy(&units))
}

fn read_chain<S: MemSource + ?Sized>(
    src: &S,
    names: &KeyNames,
    first: u64,
    depth: usize,
    budget: &mut usize,
) -> Option<Vec<KvNode>> {
    let mut out = Vec::new();
    let mut at = first;
    while at != 0 {
        if *budget == 0 || depth > MAX_DEPTH || !user_pointer(at) {
            return None;
        }
        *budget -= 1;
        let rec = src.read_vec(at, KV_SIZE);
        if rec.len() != KV_SIZE {
            return None;
        }
        let name = names.name(src, rec[KV_NAME_HIGH_AT], u16_at(&rec, KV_NAME_LOW_AT))?;
        let sub = u64_at(&rec, KV_SUB_AT);
        let number = &rec[KV_NUMBER_AT..KV_NUMBER_AT + 8];
        let value = match rec[KV_TYPE_AT] {
            0 => KvValue::Group,
            1 => {
                let p = u64_at(&rec, KV_VALUE_AT);
                KvValue::Text(if p == 0 { String::new() } else { read_cstring(src, p, MAX_STRING)? })
            }
            2 => KvValue::Int(i32::from_le_bytes([number[0], number[1], number[2], number[3]])),
            3 => KvValue::Float(f32::from_le_bytes([number[0], number[1], number[2], number[3]])),
            4 => KvValue::Pointer(u64_at(number, 0)),
            5 => {
                let p = u64_at(&rec, KV_WIDE_AT);
                KvValue::Wide(if p == 0 { String::new() } else { read_wide(src, p)? })
            }
            6 => KvValue::Color([number[0], number[1], number[2], number[3]]),
            7 => KvValue::Big(u64_at(number, 0)),
            _ => return None,
        };
        let children = if sub == 0 { Vec::new() } else { read_chain(src, names, sub, depth + 1, budget)? };
        out.push(KvNode { name, value, children });
        at = u64_at(&rec, KV_PEER_AT);
    }
    Some(out)
}

pub fn read_key_values<S: MemSource + ?Sized>(src: &S, names: &KeyNames, root: u64) -> Option<KvNode> {
    let mut budget = MAX_NODES;
    let mut list = read_chain(src, names, root, 0, &mut budget)?;
    if list.is_empty() {
        return None;
    }
    Some(list.swap_remove(0))
}

fn quoted(s: &str) -> String {
    format!("\"{s}\"")
}

fn write_node(out: &mut String, n: &KvNode, depth: usize) {
    let pad = "\t".repeat(depth);
    if n.value == KvValue::Group || !n.children.is_empty() {
        out.push_str(&format!("{pad}{}\r\n{pad}{{\r\n", quoted(&n.name)));
        for c in &n.children {
            write_node(out, c, depth + 1);
        }
        out.push_str(&format!("{pad}}}\r\n"));
        return;
    }
    let text = match &n.value {
        KvValue::Text(t) | KvValue::Wide(t) => t.clone(),
        KvValue::Int(i) => i.to_string(),
        KvValue::Float(f) => format!("{f:.6}"),
        KvValue::Color(c) => format!("{} {} {} {}", c[0], c[1], c[2], c[3]),
        KvValue::Pointer(p) => format!("{p}"),
        KvValue::Big(b) => format!("{b}"),
        KvValue::Group => String::new(),
    };
    out.push_str(&format!("{pad}{}\t\t{}\r\n", quoted(&n.name), quoted(&text)));
}

pub fn vmt_text(root: &KvNode) -> String {
    let mut out = String::new();
    write_node(&mut out, root, 0);
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemMaterial {
    pub address: u64,
    pub name: String,
    pub vmt: Option<String>,
    pub shader: Option<String>,
    pub from_variables: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemTexture {
    pub address: u64,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub format: i32,
    pub flags: u32,
    pub frames: u32,
    pub cube: bool,
    pub bytes: u64,
}

pub fn texture_bytes(width: u32, height: u32, format: i32, flags: u32, frames: u32) -> u64 {
    let Some(f) = u32::try_from(format).ok().and_then(hp_vtf::ImageFormat::from_raw) else { return 0 };
    let faces = if flags & TEXTURE_CUBE != 0 { 6u64 } else { 1 };
    let (mut w, mut h) = (width.max(1), height.max(1));
    let mut total = 0u64;
    loop {
        total += f.image_size(w, h).unwrap_or(0);
        if flags & TEXTURE_NO_MIP != 0 || (w == 1 && h == 1) {
            break;
        }
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    total * faces * u64::from(frames.max(1))
}

pub fn find_textures<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Vec<MemTexture> {
    let Some(mat) = src.module_base(MATERIAL_MODULE) else { return Vec::new() };
    let table_cell = read_u64(src, mat + MATERIAL_NAMES_CELL_RVA);
    let needle = (mat + TEXTURE_VTABLE_RVA).to_le_bytes();
    let finder = memchr::memmem::Finder::new(&needle).into_owned();
    let scan = ScanOptions { overlap: TEXTURE_SIZE, writable_only: true, ..*opts };
    let mut addrs: Vec<u64> = par_chunks(src, &scan, |base, data, own| {
        finder
            .find_iter(data)
            .filter(|&j| j < own && (base + j as u64).is_multiple_of(4))
            .map(|j| base + j as u64)
            .collect::<Vec<u64>>()
    })
    .into_iter()
    .flatten()
    .collect();
    addrs.sort_unstable();
    addrs.dedup();
    let mut out = Vec::new();
    for a in addrs {
        let rec = src.read_vec(a, TEXTURE_SIZE);
        if rec.len() != TEXTURE_SIZE {
            continue;
        }
        let Some(name) = read_symbol(src, table_cell, u16_at(&rec, TEXTURE_NAME_AT)).filter(|n| !n.is_empty()) else {
            continue;
        };
        let flags = u32_at(&rec, TEXTURE_FLAGS_AT);
        let format = u32_at(&rec, TEXTURE_FORMAT_AT) as i32;
        let (width, height) = (u32::from(u16_at(&rec, TEXTURE_WIDTH_AT)), u32::from(u16_at(&rec, TEXTURE_HEIGHT_AT)));
        let frames = u32::from(u16_at(&rec, TEXTURE_FRAMES_AT));
        out.push(MemTexture {
            address: a,
            name,
            width,
            height,
            format,
            flags,
            frames,
            cube: flags & TEXTURE_CUBE != 0,
            bytes: texture_bytes(width, height, format, flags, frames),
        });
    }
    out
}

const KNOWN_SHADERS: [&str; 6] =
    ["LightmappedGeneric", "VertexLitGeneric", "UnlitGeneric", "WorldVertexTransition", "Water", "SpriteCard"];

fn tree_resolves<S: MemSource + ?Sized>(src: &S, names: &KeyNames, root: u64) -> bool {
    let Some(tree) = read_key_values(src, names, root) else { return false };
    let mut total = 0usize;
    let mut good = 0usize;
    let mut stack: Vec<&KvNode> = vec![&tree];
    while let Some(n) = stack.pop() {
        total += 1;
        if !n.name.is_empty() && n.name.bytes().all(|b| (0x20..0x7f).contains(&b)) {
            good += 1;
        }
        stack.extend(n.children.iter());
    }
    total > 0 && good == total
}

pub fn infer_key_names<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions, roots: &[u64]) -> Option<KeyNames> {
    let sample: Vec<(u64, u64)> = roots
        .iter()
        .filter_map(|&r| {
            let rec = src.read_vec(r, KV_SIZE);
            (rec.len() == KV_SIZE)
                .then(|| (r, u64::from(rec[KV_NAME_HIGH_AT]) | (u64::from(u16_at(&rec, KV_NAME_LOW_AT)) << 8)))
        })
        .take(24)
        .collect();
    if sample.is_empty() {
        return None;
    }
    let scan = ScanOptions { overlap: 32, ..*opts };
    for shader in KNOWN_SHADERS {
        let mut needle = shader.as_bytes().to_vec();
        needle.push(0);
        let finder = memchr::memmem::Finder::new(&needle).into_owned();
        let found: Vec<u64> = par_chunks(src, &scan, |base, data, own| {
            finder.find_iter(data).filter(|&j| j < own).map(|j| base + j as u64).collect::<Vec<u64>>()
        })
        .into_iter()
        .flatten()
        .collect();
        let mut tried: std::collections::HashSet<u64> = std::collections::HashSet::new();
        for addr in found {
            for &(_, symbol) in &sample {
                let Some(pool) = addr.checked_sub(symbol) else { continue };
                if !tried.insert(pool) {
                    continue;
                }
                let names = KeyNames { strings: pool };
                let ok = sample.iter().filter(|(r, _)| tree_resolves(src, &names, *r)).count();
                if ok * 10 >= sample.len() * 8 {
                    return Some(names);
                }
            }
        }
    }
    None
}

pub fn find_key_values_system<S: MemSource + ?Sized>(src: &S, module: u64, vtable: u64) -> Option<u64> {
    let needle = vtable.to_le_bytes();
    let finder = memchr::memmem::Finder::new(&needle).into_owned();
    let mut hits: Vec<u64> = Vec::new();
    for r in src.regions() {
        if r.base + r.size <= module || r.base >= module + KEYVALUES_MODULE_REACH || !r.writable {
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
    hits.sort_unstable();
    hits.into_iter().find(|&a| read_key_names(src, a).is_some())
}

pub fn material_key_roots<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions, mat: u64) -> Vec<u64> {
    let needle = (mat + MATERIAL_VTABLE_RVA).to_le_bytes();
    let finder = memchr::memmem::Finder::new(&needle).into_owned();
    let scan = ScanOptions { overlap: MATERIAL_SIZE, writable_only: true, ..*opts };
    let addrs: Vec<u64> = par_chunks(src, &scan, |base, data, own| {
        finder
            .find_iter(data)
            .filter(|&j| j < own && (base + j as u64).is_multiple_of(4))
            .map(|j| base + j as u64)
            .collect::<Vec<u64>>()
    })
    .into_iter()
    .flatten()
    .collect();
    addrs
        .into_iter()
        .filter_map(|a| {
            let rec = src.read_vec(a, MATERIAL_SIZE);
            (rec.len() == MATERIAL_SIZE).then(|| u64_at(&rec, MATERIAL_KV_AT)).filter(|k| user_pointer(*k))
        })
        .collect()
}

pub fn find_materials<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Vec<MemMaterial> {
    let (Some(mat), Some(kv_mod)) = (src.module_base(MATERIAL_MODULE), src.module_base(KEYVALUES_MODULE)) else {
        return Vec::new();
    };
    let table_cell = read_u64(src, mat + MATERIAL_NAMES_CELL_RVA);
    let candidates = material_key_roots(src, opts, mat);
    let by_system = find_key_values_system(src, kv_mod, kv_mod + KEYVALUES_SYSTEM_VTABLE_RVA)
        .and_then(|system| read_key_names(src, system))
        .filter(|n| candidates.iter().take(8).any(|r| tree_resolves(src, n, *r)));
    let names = by_system.or_else(|| infer_key_names(src, opts, &candidates));
    let needle = (mat + MATERIAL_VTABLE_RVA).to_le_bytes();
    let finder = memchr::memmem::Finder::new(&needle).into_owned();
    let scan = ScanOptions { overlap: MATERIAL_SIZE, writable_only: true, ..*opts };
    let mut addrs: Vec<u64> = par_chunks(src, &scan, |base, data, own| {
        finder
            .find_iter(data)
            .filter(|&j| j < own && (base + j as u64).is_multiple_of(4))
            .map(|j| base + j as u64)
            .collect::<Vec<u64>>()
    })
    .into_iter()
    .flatten()
    .collect();
    addrs.sort_unstable();
    addrs.dedup();
    let tables =
        crate::matvars::SymbolTables { var_names: mat + crate::matvars::VAR_NAMES_RVA, object_names: table_cell };
    let defaults = crate::matvars::read_shader_defaults(src, &SHADER_MODULES);
    let mut out = Vec::new();
    let mut pending: Vec<(usize, Option<String>, Vec<crate::matvars::MatVar>)> = Vec::new();
    for a in addrs {
        let rec = src.read_vec(a, MATERIAL_SIZE);
        if rec.len() != MATERIAL_SIZE {
            continue;
        }
        let Some(name) = read_symbol(src, table_cell, u16_at(&rec, MATERIAL_NAME_AT)).filter(|n| !n.is_empty()) else {
            continue;
        };
        let kv = u64_at(&rec, MATERIAL_KV_AT);
        let tree = match (&names, user_pointer(kv)) {
            (Some(n), true) => read_key_values(src, n, kv),
            _ => None,
        };
        if let Some(t) = tree {
            out.push(MemMaterial {
                address: a,
                name,
                shader: Some(t.name.clone()),
                vmt: Some(vmt_text(&t)),
                from_variables: false,
            });
            continue;
        }
        let shader = crate::matvars::shader_name(src, u64_at(&rec, crate::matvars::MATERIAL_SHADER_AT));
        let vars = if shader.is_some() { crate::matvars::read_vars(src, &rec, &tables) } else { Vec::new() };
        pending.push((out.len(), shader.clone(), vars));
        out.push(MemMaterial { address: a, name, shader, vmt: None, from_variables: true });
    }
    let population: Vec<(String, Vec<crate::matvars::MatVar>)> = pending
        .iter()
        .filter(|(_, s, v)| s.is_some() && !v.is_empty())
        .map(|(_, s, v)| (s.clone().unwrap_or_default(), v.clone()))
        .collect();
    let derived = crate::matvars::derived_values(&population);
    let mut staged: Vec<(usize, String, Vec<crate::proxymem::ProxyBlock>)> = Vec::new();
    for (at, shader, vars) in pending {
        if let (Some(sh), false) = (shader, vars.is_empty()) {
            let text = crate::matvars::rebuild_vmt(&sh, &vars, &defaults, &derived);
            let rec = src.read_vec(out[at].address, MATERIAL_SIZE);
            staged.push((at, text, crate::proxymem::read_proxies(src, &rec, &tables)));
        }
    }
    let lua_keys: Vec<String> =
        staged.iter().flat_map(|(_, _, b)| b.iter()).filter_map(|b| b.lua_key.clone()).collect();
    let lua = crate::luaparam::proxy_values(src, opts, &lua_keys);
    for (at, text, mut blocks) in staged {
        for b in blocks.iter_mut() {
            if let Some(items) = b.lua_key.as_ref().and_then(|k| lua.get(k)) {
                b.keys = crate::luaparam::pairs_text(items);
                b.decoded = true;
            }
        }
        out[at].vmt = Some(crate::proxymem::insert_proxies(&text, &blocks));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    struct Builder {
        mem: VecMem,
        next: u64,
        strings: Vec<u8>,
    }

    impl Builder {
        fn new() -> Builder {
            Builder { mem: VecMem::default(), next: 0x2000_0000, strings: vec![0u8; 8] }
        }

        fn symbol(&mut self, name: &str) -> u32 {
            let at = self.strings.len() as u32;
            self.strings.extend_from_slice(name.as_bytes());
            self.strings.push(0);
            at
        }

        fn alloc(&mut self, bytes: Vec<u8>) -> u64 {
            let at = self.next;
            self.next += (bytes.len() as u64 + 0xff) & !0xff;
            self.mem.add(at, bytes);
            at
        }

        fn kv(&mut self, name: &str, ty: u8, text: Option<&str>, number: [u8; 4], peer: u64, sub: u64) -> u64 {
            let sym = self.symbol(name);
            let mut r = vec![0u8; KV_SIZE];
            r[0..3].copy_from_slice(&sym.to_le_bytes()[0..3]);
            r[KV_NAME_HIGH_AT] = (sym & 0xff) as u8;
            r[KV_NAME_LOW_AT..KV_NAME_LOW_AT + 2].copy_from_slice(&((sym >> 8) as u16).to_le_bytes());
            r[KV_TYPE_AT] = ty;
            if let Some(t) = text {
                let mut s = t.as_bytes().to_vec();
                s.push(0);
                let p = self.alloc(s);
                r[KV_VALUE_AT..KV_VALUE_AT + 8].copy_from_slice(&p.to_le_bytes());
            }
            r[KV_NUMBER_AT..KV_NUMBER_AT + 4].copy_from_slice(&number);
            r[KV_PEER_AT..KV_PEER_AT + 8].copy_from_slice(&peer.to_le_bytes());
            r[KV_SUB_AT..KV_SUB_AT + 8].copy_from_slice(&sub.to_le_bytes());
            self.alloc(r)
        }

        fn finish_names(&mut self) -> u64 {
            let taken = std::mem::take(&mut self.strings);
            let strings = self.alloc(taken);
            let mut system = vec![0u8; 0x80];
            system[KV_SYSTEM_STRINGS_AT as usize..KV_SYSTEM_STRINGS_AT as usize + 8]
                .copy_from_slice(&strings.to_le_bytes());
            self.alloc(system)
        }
    }

    #[test]
    fn a_patch_material_is_written_back_as_the_compiler_wrote_it() {
        let mut b = Builder::new();
        let envmap = b.kv("$envmap", 1, Some("maps/x/c1_2_3"), [0; 4], 0, 0);
        let replace = b.kv("replace", 0, None, [0; 4], 0, envmap);
        let include = b.kv("include", 1, Some("materials/CONCRETE/FLOOR.vmt"), [0; 4], replace, 0);
        let root = b.kv("patch", 0, None, [0; 4], 0, include);
        let system = b.finish_names();
        let names = read_key_names(&b.mem, system).expect("names");
        let tree = read_key_values(&b.mem, &names, root).expect("tree");
        assert_eq!(tree.name, "patch");
        let text = vmt_text(&tree);
        assert_eq!(
            text,
            "\"patch\"\r\n{\r\n\t\"include\"\t\t\"materials/CONCRETE/FLOOR.vmt\"\r\n\t\"replace\"\r\n\t{\r\n\t\t\"$envmap\"\t\t\"maps/x/c1_2_3\"\r\n\t}\r\n}\r\n"
        );
    }

    #[test]
    fn numbers_and_colors_are_written_like_the_engine_does() {
        let mut b = Builder::new();
        let c = b.kv("$color", 6, None, [10, 20, 30, 255], 0, 0);
        let f = b.kv("$scale", 3, None, 0.5f32.to_le_bytes(), c, 0);
        let i = b.kv("$flag", 2, None, 7i32.to_le_bytes(), f, 0);
        let root = b.kv("LightmappedGeneric", 0, None, [0; 4], 0, i);
        let system = b.finish_names();
        let names = read_key_names(&b.mem, system).expect("names");
        let text = vmt_text(&read_key_values(&b.mem, &names, root).expect("tree"));
        assert!(text.contains("\"$flag\"\t\t\"7\""));
        assert!(text.contains("\"$scale\"\t\t\"0.500000\""));
        assert!(text.contains("\"$color\"\t\t\"10 20 30 255\""));
    }

    #[test]
    fn a_symbol_is_resolved_through_the_element_and_pool_tables() {
        let mut m = VecMem::default();
        let table = 0x1000_0000u64;
        let mut t = vec![0u8; 0x80];
        t[8..16].copy_from_slice(&0x1000_1000u64.to_le_bytes());
        t[0x38..0x40].copy_from_slice(&0x1000_2000u64.to_le_bytes());
        m.add(table, t);
        let mut elements = vec![0u8; 36];
        elements[12 + 8..12 + 10].copy_from_slice(&1u16.to_le_bytes());
        elements[12 + 10..12 + 12].copy_from_slice(&4u16.to_le_bytes());
        m.add(0x1000_1000, elements);
        let mut pools = vec![0u8; 16];
        pools[8..16].copy_from_slice(&0x1000_3000u64.to_le_bytes());
        m.add(0x1000_2000, pools);
        let mut pool = vec![0u8; 64];
        pool[14..14 + 11].copy_from_slice(b"maps/a/wall");
        m.add(0x1000_3000, pool);
        assert_eq!(read_symbol(&m, table, 1).as_deref(), Some("maps/a/wall"));
        assert!(read_symbol(&m, table, u16::MAX).is_none());
    }

    #[test]
    fn materials_are_found_by_their_vtable_with_their_loaded_key_values() {
        let mut b = Builder::new();
        let color = b.kv("$basetexture", 1, Some("gm_construct/grass1"), [0; 4], 0, 0);
        let root = b.kv("LightmappedGeneric", 0, None, [0; 4], 0, color);
        let system = b.finish_names();
        let mut m = b.mem;
        let mat_base = 0x3000_0000u64;
        let kv_base = 0x3100_0000u64;
        m.modules.push((MATERIAL_MODULE.to_owned(), mat_base));
        m.modules.push((KEYVALUES_MODULE.to_owned(), kv_base));
        let mut sys_obj = vec![0u8; 0x80];
        sys_obj[0..8].copy_from_slice(&(kv_base + KEYVALUES_SYSTEM_VTABLE_RVA).to_le_bytes());
        let strings = read_u64(&m, system + KV_SYSTEM_STRINGS_AT);
        sys_obj[KV_SYSTEM_STRINGS_AT as usize..KV_SYSTEM_STRINGS_AT as usize + 8]
            .copy_from_slice(&strings.to_le_bytes());
        m.add(kv_base + 0x2000, sys_obj);
        let table = 0x5000_0000u64;
        let mut t = vec![0u8; 0x80];
        t[8..16].copy_from_slice(&0x5000_1000u64.to_le_bytes());
        t[0x38..0x40].copy_from_slice(&0x5000_2000u64.to_le_bytes());
        m.add(table, t);
        let mut elements = vec![0u8; 24];
        elements[12 + 8..12 + 10].copy_from_slice(&0u16.to_le_bytes());
        elements[12 + 10..12 + 12].copy_from_slice(&0u16.to_le_bytes());
        m.add(0x5000_1000, elements);
        m.add(0x5000_2000, 0x5000_3000u64.to_le_bytes().to_vec());
        let mut pool = vec![0u8; 64];
        pool[10..10 + 12].copy_from_slice(b"concrete/abc");
        m.add(0x5000_3000, pool);
        let mut cell = vec![0u8; 8];
        cell.copy_from_slice(&table.to_le_bytes());
        m.add(mat_base + MATERIAL_NAMES_CELL_RVA, cell);
        let mut obj = vec![0u8; MATERIAL_SIZE];
        obj[0..8].copy_from_slice(&(mat_base + MATERIAL_VTABLE_RVA).to_le_bytes());
        obj[MATERIAL_NAME_AT..MATERIAL_NAME_AT + 2].copy_from_slice(&1u16.to_le_bytes());
        obj[MATERIAL_KV_AT..MATERIAL_KV_AT + 8].copy_from_slice(&root.to_le_bytes());
        m.add(0x6000_0000, obj);
        let found = find_materials(&m, &ScanOptions::default());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "concrete/abc");
        assert_eq!(found[0].shader.as_deref(), Some("LightmappedGeneric"));
        assert!(found[0].vmt.as_deref().expect("vmt").contains("\"$basetexture\"\t\t\"gm_construct/grass1\""));
    }

    #[test]
    fn a_broken_chain_is_refused_instead_of_looping() {
        let mut b = Builder::new();
        let a = b.kv("a", 1, Some("x"), [0; 4], 0, 0);
        let mut rec = b.mem.read_vec(a, KV_SIZE);
        rec[KV_PEER_AT..KV_PEER_AT + 8].copy_from_slice(&a.to_le_bytes());
        b.mem.blocks.iter_mut().find(|(base, _)| *base == a).expect("block").1 = rec;
        let system = b.finish_names();
        let names = read_key_names(&b.mem, system).expect("names");
        assert!(read_key_values(&b.mem, &names, a).is_none());
    }

    #[test]
    fn the_size_of_a_texture_follows_its_format_mips_and_faces() {
        assert_eq!(texture_bytes(32, 32, 13, TEXTURE_CUBE, 1), 6 * (512 + 128 + 32 + 8 + 8 + 8));
        assert_eq!(texture_bytes(4, 4, 13, TEXTURE_NO_MIP, 1), 8);
        assert_eq!(texture_bytes(4, 4, 12, TEXTURE_NO_MIP, 2), 128);
        assert_eq!(texture_bytes(4, 4, 999, 0, 1), 0);
    }

    #[test]
    fn the_key_name_pool_is_inferred_from_a_known_shader_name_when_the_system_is_not_found() {
        let mut b = Builder::new();
        let tex = b.kv("$basetexture", 1, Some("a/b"), [0; 4], 0, 0);
        let root = b.kv("LightmappedGeneric", 0, None, [0; 4], 0, tex);
        let tex2 = b.kv("$color", 1, Some("[1 1 1]"), [0; 4], 0, 0);
        let root2 = b.kv("VertexLitGeneric", 0, None, [0; 4], 0, tex2);
        let system = b.finish_names();
        let pool = read_u64(&b.mem, system + KV_SYSTEM_STRINGS_AT);
        let found = infer_key_names(&b.mem, &ScanOptions::default(), &[root, root2]).expect("pool");
        assert_eq!(found.strings, pool);
        assert!(infer_key_names(&b.mem, &ScanOptions::default(), &[]).is_none());
    }
}
