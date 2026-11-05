use ahash::{AHashMap, AHashSet};
use glam::DVec3;

use crate::materials::{MaterialFacts, MaterialSource};
use crate::winding::Winding;

mod sky;

pub const MAX_SAMPLES: usize = 1024;
pub const TEXTURE_NAME_LENGTH: usize = 128;
const DEFAULT_SIZE: u32 = 32;
const DEPENDENT_VARS: [&str; 3] = ["$bottommaterial", "$crackmaterial", "$fallbackmaterial"];
const MAX_DEPENDENT_DEPTH: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub origin: [i32; 3],
    pub size: i32,
}

#[derive(Clone, Debug, Default)]
struct MaterialInfo {
    env_cubemap: bool,
    dependent: Option<(String, String)>,
    raw: Option<hp_kv::Block>,
    flattened: Option<hp_vmt::Material>,
}

pub struct Cubemaps<'a> {
    map: String,
    samples: Vec<Sample>,
    manual: AHashMap<u32, (usize, bool)>,
    manual_list: Vec<(usize, u32)>,
    seen: AHashSet<u32>,
    source: &'a dyn MaterialSource,
    infos: AHashMap<String, MaterialInfo>,
    patched: AHashMap<(String, usize), Option<String>>,
    files: Vec<(String, Vec<u8>)>,
    file_index: AHashMap<String, usize>,
    pub messages: Vec<String>,
    inserted: usize,
    used: Vec<usize>,
    parallax_names: Vec<String>,
    parallax: Vec<Option<String>>,
}

pub const PARALLAX_OBB_CLASS: &str = "parallax_obb";

pub fn parallax_obb_matrix(windings: &[Option<Vec<glam::Vec3>>]) -> [[f64; 4]; 3] {
    let identity = [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]];
    let valid: Vec<&Vec<glam::Vec3>> = windings.iter().flatten().filter(|w| w.len() >= 3).collect();
    let (Some(first), Some(third)) = (valid.first(), valid.get(2)) else { return identity };
    if first.len() < 4 {
        return identity;
    }
    let corner = first[0];
    let y = first[1] - corner;
    let z = first[3] - corner;
    let x = y.cross(z).normalize_or_zero();
    let diag = third[0] - third[2];
    let x = x * diag.dot(x).abs();
    let m = glam::DMat3::from_cols(x.as_dvec3(), y.as_dvec3(), z.as_dvec3());
    if m.determinant().abs() < 1e-12 {
        return identity;
    }
    let inv = m.inverse();
    let t = -(inv * corner.as_dvec3());
    let r = |i: usize| [inv.row(i).x, inv.row(i).y, inv.row(i).z, t[i]];
    [r(0), r(1), r(2)]
}

pub fn parallax_rows(m: &[[f64; 4]; 3]) -> String {
    m.iter()
        .map(|r| format!("[{:.6} {:.6} {:.6} {:.6}]", r[0] as f32, r[1] as f32, r[2] as f32, r[3] as f32))
        .collect::<Vec<_>>()
        .join(";")
}

fn lower_name(name: &str) -> String {
    name.replace('\\', "/").to_ascii_lowercase()
}

pub(crate) fn atoi(s: &str) -> i32 {
    let s = s.trim_start();
    let (neg, digits) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let mut v: i64 = 0;
    for c in digits.chars() {
        let Some(d) = c.to_digit(10) else { break };
        v = (v * 10 + i64::from(d)).min(i64::from(i32::MAX) + 1);
    }
    let v = if neg { -v } else { v };
    v.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}

fn sample_origin(s: &str) -> [i32; 3] {
    let mut v = [0.0f32; 3];
    for (k, part) in s.split_whitespace().take(3).enumerate() {
        v[k] = part.parse::<f64>().map_or(0.0, |x| x as f32);
    }
    v.map(|x| if x.is_finite() { x as i32 } else { 0 })
}

fn text_of(bytes: &[u8]) -> String {
    hp_kv::decode(bytes).0.into_owned()
}

#[derive(Default)]
struct PatchBlock {
    props: Vec<(String, String)>,
    children: Vec<(String, PatchBlock)>,
}

impl PatchBlock {
    fn set(&mut self, key: &str, value: &str) {
        match self.props.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some(p) => p.1 = value.to_owned(),
            None => self.props.push((key.to_owned(), value.to_owned())),
        }
    }

    fn child(&mut self, name: &str) -> &mut PatchBlock {
        let at = match self.children.iter().position(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(i) => i,
            None => {
                self.children.push((name.to_owned(), PatchBlock::default()));
                self.children.len() - 1
            }
        };
        &mut self.children[at].1
    }

    fn write(&self, name: &str, indent: usize, out: &mut String) {
        let tabs = "\t".repeat(indent);
        out.push_str(&format!("{tabs}\"{name}\"\n{tabs}{{\n"));
        for (k, v) in self.props.iter().filter(|(_, v)| !v.is_empty()) {
            out.push_str(&format!("{tabs}\t\"{k}\"\t\t\"{v}\"\n"));
        }
        for (n, c) in self.children.iter().filter(|(_, c)| !c.props.is_empty() || !c.children.is_empty()) {
            c.write(n, indent + 1, out);
        }
        out.push_str(&format!("{tabs}}}\n"));
    }
}

type PatchKey<'s> = (&'s str, Option<&'s str>, &'s str);

fn mirror(original: &hp_kv::Block, out: &mut PatchBlock, keys: &[PatchKey<'_>]) {
    for (key, required, value) in keys {
        let Some(v) = original.get(key) else { continue };
        if required.is_some_and(|r| !v.trim().eq_ignore_ascii_case(r)) {
            continue;
        }
        out.set(key, value);
    }
    for child in &original.children {
        mirror(child, out.child(&child.name), keys);
    }
}

fn replace_everywhere(block: &mut hp_kv::Block, keys: &[PatchKey<'_>]) {
    for (key, required, value) in keys {
        for (k, v) in &mut block.props {
            if k.eq_ignore_ascii_case(key) && required.is_none_or(|r| v.trim().eq_ignore_ascii_case(r)) {
                *v = (*value).to_owned();
            }
        }
    }
    for child in &mut block.children {
        replace_everywhere(child, keys);
    }
}

impl<'a> Cubemaps<'a> {
    pub fn new(entities: &[&hp_vmf::Entity], map_name: Option<&str>, source: &'a dyn MaterialSource) -> Self {
        let mut c = Cubemaps {
            map: map_name.map(lower_name).unwrap_or_default(),
            samples: Vec::new(),
            manual: AHashMap::new(),
            manual_list: Vec::new(),
            seen: AHashSet::new(),
            source,
            infos: AHashMap::new(),
            patched: AHashMap::new(),
            inserted: 0,
            used: Vec::new(),
            files: Vec::new(),
            file_index: AHashMap::new(),
            messages: Vec::new(),
            parallax_names: Vec::new(),
            parallax: Vec::new(),
        };
        for e in entities.iter().skip(1).filter(|e| e.classname.eq_ignore_ascii_case("env_cubemap")) {
            if c.samples.len() + 1 >= MAX_SAMPLES {
                c.messages
                    .push(format!("Error: too many env_cubemap entities (max {MAX_SAMPLES}), the others are left out"));
                break;
            }
            let index = c.samples.len();
            c.samples.push(Sample {
                origin: sample_origin(e.get("origin").unwrap_or("")),
                size: atoi(e.get("cubemapsize").unwrap_or("0")),
            });
            c.parallax_names.push(e.get("parallaxobb").unwrap_or("").trim().to_owned());
            c.parallax.push(None);
            for id in e.get("sides").unwrap_or("").split_whitespace() {
                let Ok(id) = id.parse::<i64>() else { continue };
                let id = id as u32;
                c.manual.entry(id).and_modify(|m| m.1 = true).or_insert((index, false));
                c.manual_list.push((index, id));
            }
        }
        if c.map.is_empty() && !c.samples.is_empty() {
            c.messages.push("Warning: no map name given, env_cubemap materials are not patched".to_owned());
        }
        c
    }

    pub fn resolve_parallax(&mut self, obbs: &[(String, String)]) {
        for (i, name) in self.parallax_names.iter().enumerate() {
            if name.is_empty() {
                continue;
            }
            match obbs.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
                Some((_, rows)) => self.parallax[i] = Some(rows.clone()),
                None => self
                    .messages
                    .push(format!("Warning: Cannot find parallax obb \"{name}\" (num_entities is {})", obbs.len())),
            }
        }
    }

    pub fn parallax_of(&self, sample: usize) -> Option<&str> {
        self.parallax.get(sample).and_then(|p| p.as_deref())
    }

    pub fn samples(&self) -> &[Sample] {
        &self.samples
    }

    pub fn lump(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.samples.len() * 16);
        for s in &self.samples {
            for v in s.origin {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&s.size.to_le_bytes());
        }
        out
    }

    pub fn side_material(
        &mut self,
        material: &str,
        facts: &MaterialFacts,
        side_id: u32,
        winding: Option<&Winding>,
        offset: DVec3,
        normal: DVec3,
    ) -> Option<String> {
        self.seen.insert(side_id);
        if self.samples.is_empty() || self.map.is_empty() {
            return None;
        }
        let uses = facts.envmap.is_some() || self.info(material, Some(facts)).dependent.is_some();
        if !uses {
            return None;
        }
        let (sample, multiple) = match self.manual.get(&side_id) {
            Some(&(s, m)) => (s, m),
            None => (self.nearest(winding, offset, normal), false),
        };
        let name = self.patch(material, sample, 0);
        self.note_used(sample, name.is_some());
        if multiple && name.is_some() {
            self.messages.push(format!("Multiple references for cubemap on texture {material}!!!"));
        }
        name
    }

    pub fn dependent_name(&self, patched: &str, dependent: &str) -> String {
        let key = lower_name(patched);
        let sample = self.patched.iter().find(|(_, v)| v.as_deref() == Some(key.as_str())).map(|((_, s), _)| *s);
        sample
            .and_then(|s| self.patched.get(&(lower_name(dependent), s)).cloned().flatten())
            .unwrap_or_else(|| dependent.to_owned())
    }

    pub fn manual_sides(&self) -> Vec<(usize, u32)> {
        self.manual_list.clone()
    }

    pub fn is_manual(&self, side_id: u32) -> bool {
        self.manual.contains_key(&side_id)
    }

    pub fn manual_material(&mut self, material: &str, side_id: u32, sample: usize) -> Option<String> {
        self.seen.insert(side_id);
        if self.samples.is_empty() || self.map.is_empty() || sample >= self.samples.len() {
            return None;
        }
        let name = self.patch(material, sample, 0);
        self.note_used(sample, name.is_some());
        name
    }

    fn note_used(&mut self, sample: usize, patched: bool) {
        if patched && !self.used.contains(&sample) {
            self.used.push(sample);
        }
    }

    pub fn wvt_patch(&mut self, original: &str, patched: &str) -> bool {
        let path = format!("materials/{}.vmt", hp_vmt::normalize_material_name(original));
        let Some(raw) = self
            .source
            .read_file(&path)
            .and_then(|b| hp_kv::parse(&text_of(&b)).ok())
            .and_then(|r| r.children.into_iter().next())
        else {
            return false;
        };
        fn copy(b: &hp_kv::Block) -> PatchBlock {
            PatchBlock {
                props: b.props.iter().map(|(k, v)| (k.clone(), kv_typed_value(v))).collect(),
                children: b.children.iter().map(|c| (c.name.clone(), copy(c))).collect(),
            }
        }
        let mut root = copy(&raw);
        let mut remove = |key: &str| {
            if let Some(i) = root.props.iter().position(|(k, _)| k.eq_ignore_ascii_case(key)) {
                root.props.remove(i);
            }
        };
        for key in [
            "$basetexture2",
            "$bumpmap2",
            "$bumpframe2",
            "$basetexture2noenvmap",
            "$blendmodulatetexture",
            "$maskedblending",
            "$surfaceprop2",
        ] {
            remove(key);
        }
        if raw.get("$basetexturenoenvmap").is_some_and(|v| atoi(v) != 0) {
            remove("$envmap");
        }
        let mut text = String::new();
        root.write("LightmappedGeneric", 0, &mut text);
        let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        self.add_file(&format!("materials/{patched}.vmt"), text.into_bytes(), true);
        true
    }

    pub fn map_name(&self) -> &str {
        &self.map
    }

    pub fn insert_patch(&mut self, original: &str, patched: &str, key: &str, value: &str) -> bool {
        if self.map.is_empty() {
            return false;
        }
        let mut insert = PatchBlock::default();
        insert.set(key, value);
        let mut root = PatchBlock::default();
        root.set("include", &format!("materials/{}.vmt", crate::materials::material_path(original)));
        root.children.push(("insert".to_owned(), insert));
        let mut text = String::new();
        root.write("patch", 0, &mut text);
        let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        self.add_file(&format!("materials/{patched}.vmt"), text.into_bytes(), true);
        self.inserted += 1;
        true
    }

    pub fn check_sides(&mut self) {
        let mut missing: Vec<(u32, usize)> =
            self.manual.iter().filter(|(id, _)| !self.seen.contains(id)).map(|(id, (s, _))| (*id, *s)).collect();
        missing.sort_unstable();
        for (_, s) in missing {
            let o = self.samples[s].origin;
            self.messages
                .push(format!("env_cubemap pointing at deleted brushside near ({}, {}, {})", o[0], o[1], o[2]));
        }
    }

    fn nearest(&self, winding: Option<&Winding>, offset: DVec3, normal: DVec3) -> usize {
        let Some(w) = winding.filter(|w| !w.points.is_empty()) else { return 0 };
        let mut c = [0.0f32; 3];
        for p in &w.points {
            c[0] += p.x as f32;
            c[1] += p.y as f32;
            c[2] += p.z as f32;
        }
        let rn = 1.0 / w.points.len() as f32;
        let c = [c[0] * rn + offset.x as f32, c[1] * rn + offset.y as f32, c[2] * rn + offset.z as f32];
        let nr = [normal.x as f32, normal.y as f32, normal.z as f32];
        let delta = |s: &Sample| [s.origin[0] as f32 - c[0], s.origin[1] as f32 - c[1], s.origin[2] as f32 - c[2]];
        let length = |d: [f32; 3]| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let (mut best, mut min) = (None, f32::MAX);
        for (i, s) in self.samples.iter().enumerate() {
            let d = delta(s);
            let (u, dist) = crate::planes::normalize_f32(glam::Vec3::from(d));
            let dot = u.x * nr[0] + u.y * nr[1] + u.z * nr[2];
            if dot >= 0.0 && dist < min {
                min = dist;
                best = Some(i);
            }
        }
        if let Some(b) = best {
            return b;
        }
        for (i, s) in self.samples.iter().enumerate() {
            let dist = length(delta(s));
            if dist < min {
                min = dist;
                best = Some(i);
            }
        }
        best.unwrap_or(0)
    }

    fn info(&mut self, material: &str, facts: Option<&MaterialFacts>) -> &MaterialInfo {
        let key = lower_name(material);
        if !self.infos.contains_key(&key) {
            let info = self.load_info(material, facts);
            self.infos.insert(key.clone(), info);
        }
        &self.infos[&key]
    }

    fn load_info(&mut self, material: &str, facts: Option<&MaterialFacts>) -> MaterialInfo {
        let envmap = match facts {
            Some(f) => f.envmap.clone(),
            None => self.source.facts(material).envmap,
        };
        let mut info = MaterialInfo {
            env_cubemap: envmap.is_some_and(|e| e.eq_ignore_ascii_case("env_cubemap")),
            ..MaterialInfo::default()
        };
        let path = format!("materials/{}.vmt", hp_vmt::normalize_material_name(material));
        let Some(raw) = self
            .source
            .read_file(&path)
            .and_then(|b| hp_kv::parse(&text_of(&b)).ok())
            .and_then(|r| r.children.into_iter().next())
        else {
            return info;
        };
        let mut resolved = hp_vmt::Material::from_block(raw.clone());
        if resolved.is_patch() {
            let source = self.source;
            let load = |p: &str| source.read_file(p).map(|b| text_of(&b));
            if let Ok(m) = hp_vmt::resolve_patch(&resolved, &load) {
                info.flattened = Some(m.clone());
                resolved = m;
            }
        }
        let resolved = resolved.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
        for var in DEPENDENT_VARS {
            let Some(dep) = resolved.get(var).map(str::trim).filter(|d| !d.is_empty()) else { continue };
            if lower_name(dep) == lower_name(material) {
                self.messages
                    .push(format!("Material {material} is depending on itself through materialvar {var}! Ignoring..."));
                continue;
            }
            info.dependent = Some((var.to_owned(), dep.to_owned()));
            break;
        }
        info.raw = Some(raw);
        info
    }

    fn patched_name(&self, name: &str, origin: [i32; 3]) -> String {
        lower_name(&format!(
            "maps/{}/{}_{}_{}_{}",
            self.map,
            crate::materials::material_path(name),
            origin[0],
            origin[1],
            origin[2]
        ))
    }

    fn patch(&mut self, material: &str, sample: usize, depth: usize) -> Option<String> {
        let memo = (lower_name(material), sample);
        if let Some(done) = self.patched.get(&memo) {
            return done.clone();
        }
        let result = self.make_patch(material, sample, depth);
        self.patched.insert(memo, result.clone());
        result
    }

    fn make_patch(&mut self, material: &str, sample: usize, depth: usize) -> Option<String> {
        let info = self.info(material, None).clone();
        let origin = self.samples[sample].origin;
        let dependent = match &info.dependent {
            Some((var, dep)) if depth < MAX_DEPENDENT_DEPTH => {
                self.patch(dep, sample, depth + 1).map(|p| (var.clone(), p))
            }
            _ => None,
        };
        if !info.env_cubemap && dependent.is_none() {
            return None;
        }
        let name = material.replace('\\', "/");
        let patched = self.patched_name(&name, origin);
        if patched.len() >= TEXTURE_NAME_LENGTH - 1 {
            self.messages.push(format!(
                "Error: generated env_cubemap patch name : {patched} too long! (max = {TEXTURE_NAME_LENGTH}), {material} keeps env_cubemap"
            ));
            return None;
        }
        let cubemap = lower_name(&format!("maps/{}/c{}_{}_{}", self.map, origin[0], origin[1], origin[2]));
        let mut keys: Vec<PatchKey<'_>> = Vec::new();
        if info.env_cubemap {
            keys.push(("$envmap", Some("env_cubemap"), cubemap.as_str()));
        }
        let parallax = self.parallax_of(sample).map(str::to_owned);
        let rows: Vec<String> =
            parallax.as_deref().map(|p| p.split(';').map(str::to_owned).collect()).unwrap_or_default();
        let origin_row = format!("[{} {} {}]", origin[0], origin[1], origin[2]);
        if rows.len() == 3 {
            keys.push(("$envMapParallax", None, "1"));
            keys.push(("$envMapParallaxOBB1", None, rows[0].as_str()));
            keys.push(("$envMapParallaxOBB2", None, rows[1].as_str()));
            keys.push(("$envMapParallaxOBB3", None, rows[2].as_str()));
            keys.push(("$envMapOrigin", None, origin_row.as_str()));
        }
        if let Some((var, p)) = &dependent {
            keys.push((var.as_str(), None, p.as_str()));
        }
        let text = if rows.len() == 3 && info.flattened.is_none() {
            let mut insert = PatchBlock::default();
            for (k, _, v) in &keys {
                insert.set(k, v);
            }
            let mut root = PatchBlock::default();
            root.set("include", &format!("materials/{}.vmt", keep_separators(material)));
            root.children.push(("insert".to_owned(), insert));
            let mut s = String::new();
            root.write("patch", 0, &mut s);
            s
        } else if let Some(flat) = &info.flattened {
            let mut block = flat.to_block();
            replace_everywhere(&mut block, &keys);
            if rows.len() == 3 {
                for (k, _, v) in &keys {
                    if !block.props.iter().any(|(name, _)| name.eq_ignore_ascii_case(k)) {
                        block.props.push(((*k).to_owned(), (*v).to_owned()));
                    }
                }
            }
            hp_vmt::write(&hp_vmt::Material::from_block(block))
        } else {
            let mut replace = PatchBlock::default();
            match &info.raw {
                Some(raw) => mirror(raw, &mut replace, &keys),
                None => {
                    for (k, _, v) in &keys {
                        replace.set(k, v);
                    }
                }
            }
            let mut root = PatchBlock::default();
            root.set("include", &format!("materials/{}.vmt", keep_separators(material)));
            root.children.push(("replace".to_owned(), replace));
            let mut s = String::new();
            root.write("patch", 0, &mut s);
            s
        };
        let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
        self.add_file(&format!("materials/{patched}.vmt"), text.into_bytes(), true);
        Some(patched)
    }

    fn add_file(&mut self, path: &str, data: Vec<u8>, replace: bool) {
        let key = lower_name(path);
        match self.file_index.get(&key) {
            Some(&i) if replace => self.files[i].1 = data,
            Some(_) => {}
            None => {
                self.file_index.insert(key, self.files.len());
                self.files.push((path.to_owned(), data));
            }
        }
    }

    pub fn resolve_sky(&mut self, requested: &str) -> String {
        let requested = requested.trim();
        if requested.eq_ignore_ascii_case("painted") || sky::renderable(self.source, requested) {
            return requested.to_owned();
        }
        for fallback in ["sky_day01_01", "sky_borealis01", "sky_day01_09"] {
            if !fallback.eq_ignore_ascii_case(requested) && sky::renderable(self.source, fallback) {
                self.messages.push(format!(
                    "Warning: skybox fallback: skybox/{requested} has missing or invalid material/texture files; using {fallback} in the compiled map. Mount the requested sky's content to keep its appearance. The source VMF is unchanged."
                ));
                return fallback.to_owned();
            }
        }
        requested.to_owned()
    }

    fn sky_format(&self, sky: &str, hdr: bool) -> Option<(hp_vtf::ImageFormat, u32, u32, u32)> {
        let mut first: Option<hp_vtf::VtfInfo> = None;
        let mut union = 0u32;
        for face in ["rt", "lf", "bk", "ft", "up", "dn"] {
            let info = sky::face(self.source, sky, face, hdr).or_else(|| sky::face(self.source, sky, face, !hdr))?;
            union |= info.flags;
            let no_alpha = |f: u32| f & !(hp_vtf::flags::EIGHTBITALPHA | hp_vtf::flags::ONEBITALPHA);
            if let Some(f) = &first {
                let width_ok = info.width == f.width || info.width == 4;
                let height_ok = info.height == f.height || info.height == f.height * 2 || info.height == 4;
                if !width_ok || !height_ok || no_alpha(info.flags) != no_alpha(f.flags) {
                    return None;
                }
            } else {
                first = Some(info);
            }
        }
        let f = first?;
        Some((f.format, union, f.frames.max(1), f.flags))
    }

    pub fn finish(&mut self, sky: Option<&str>) -> (Vec<u8>, Vec<(String, Vec<u8>)>) {
        if !self.map.is_empty() {
            let sky = sky.map(str::trim).unwrap_or("");
            let formats = [self.sky_format(sky, false), self.sky_format(sky, true)];
            if formats.iter().all(Option::is_none) && !sky.eq_ignore_ascii_case("painted") {
                self.messages.push(format!("Warning: Can't load skybox file skybox/{sky} to build the default cubemap! Check the mounted sky materials and textures. This warning does not disable baked lighting."));
            }
            let mut names: Vec<String> = Vec::new();
            for s in self.used.iter().copied().chain(0..self.samples.len()) {
                let o = self.samples[s].origin;
                let name = format!("materials/maps/{}/c{}_{}_{}", self.map, o[0], o[1], o[2]);
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            for hdr in [false, true] {
                let Some((ldr_format, flags, frames, _)) = formats[usize::from(hdr)] else {
                    continue;
                };
                self.messages.push(format!(
                    "Creating default {}cubemaps for env_cubemap using skybox materials:\n   skybox/{sky}*.vmt\n ! Run buildcubemaps in the engine to get the correct cube maps.",
                    if hdr { "HDR " } else { "LDR " }
                ));
                let format = if hdr { hp_vtf::ImageFormat::from_raw(24).unwrap_or(ldr_format) } else { ldr_format };
                let flags = if hdr { flags & !SRGB } else { flags };
                let data = black_cubemap(format, flags | hp_vtf::flags::ENVMAP, frames, DEFAULT_SIZE);
                let ext = if hdr { "hdr.vtf" } else { "vtf" };
                self.add_file(&format!("materials/maps/{}/cubemapdefault.{ext}", self.map), data.clone(), false);
                for name in &names {
                    self.add_file(&format!("{name}.{ext}"), data.clone(), false);
                }
            }
        }
        (self.lump(), std::mem::take(&mut self.files))
    }
}

const SRGB: u32 = 0x40;

fn kv_typed_value(value: &str) -> String {
    let b = value.as_bytes();
    let skip_ws = |mut i: usize| {
        while i < b.len() && matches!(b[i], b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r') {
            i += 1;
        }
        i
    };
    let digits = |mut i: usize| {
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        (i, i > start)
    };
    let sign = |i: usize| if i < b.len() && matches!(b[i], b'+' | b'-') { i + 1 } else { i };
    if b.is_empty() || (b.len() == 18 && b.starts_with(b"0x")) {
        return value.to_owned();
    }
    let s = sign(skip_ws(0));
    let (int_end, int_any) = digits(s);
    let int_end = if int_any { int_end } else { 0 };
    let (mut f, whole) = digits(s);
    let mut any = whole;
    if f < b.len() && b[f] == b'.' {
        let (g, frac) = digits(f + 1);
        if whole || frac {
            f = g;
            any = true;
        }
    }
    let float_end = if any {
        if f < b.len() && matches!(b[f], b'e' | b'E') {
            let (g, exp) = digits(sign(f + 1));
            if exp {
                f = g;
            }
        }
        f
    } else {
        0
    };
    if float_end > int_end && float_end == b.len() {
        if let Ok(v) = value.trim_start().parse::<f64>() {
            return format!("{:.6}", v as f32 as f64);
        }
    } else if int_end == b.len()
        && let Ok(v) = value.trim_start().trim_start_matches('+').parse::<i64>()
        && let Ok(v) = i32::try_from(v)
    {
        return v.to_string();
    }
    value.to_owned()
}

pub fn emit(cubemaps: &mut Cubemaps<'_>, lumps: &mut crate::emit::Lumps, sky: Option<&str>, log: &mut dyn FnMut(&str)) {
    use crate::emit::lump;
    cubemaps.check_sides();
    let (data, files) = cubemaps.finish(sky);
    for m in cubemaps.messages.drain(..) {
        log(&m);
    }
    let samples = cubemaps.samples().len();
    lumps.data.insert(lump::CUBEMAPS, (data, 0));
    if samples > 0 {
        let patched = files.iter().filter(|(n, _)| n.ends_with(".vmt")).count().saturating_sub(cubemaps.inserted);
        log(&format!("{samples} cubemap samples, {patched} materials patched for them"));
    }
    if files.is_empty() {
        return;
    }
    let existing = lumps.data.get(&lump::PAKFILE).map(|(d, _)| d.clone()).filter(|d| !d.is_empty());
    let mut writer = match existing {
        Some(d) => match hp_bsp::PakArchive::parse(d).and_then(|p| p.to_writer()) {
            Ok(w) => w,
            Err(e) => {
                log(&format!("Warning: the pakfile cannot be read ({e}), it is rebuilt"));
                hp_bsp::PakWriter::new()
            }
        },
        None => hp_bsp::PakWriter::new(),
    };
    for (name, data) in files {
        if let Err(e) = writer.add(&name, data) {
            log(&format!("Error: {name} cannot go in the pakfile: {e}"));
        }
    }
    match writer.finish_vbsp() {
        Ok(zip) => {
            lumps.data.insert(lump::PAKFILE, (zip, 0));
        }
        Err(e) => log(&format!("Error: the pakfile cannot be written: {e}")),
    }
}

pub fn black_cubemap(format: hp_vtf::ImageFormat, flags: u32, frames: u32, size: u32) -> Vec<u8> {
    const HEADER: u32 = 88;
    let size = size.max(1);
    let mips = 32 - size.leading_zeros();
    let frames = frames.clamp(1, 64);
    let faces = 6u64;
    let mut len = 0u64;
    for m in 0..mips {
        let side = (size >> m).max(1);
        len += format.image_size(side, side).unwrap_or(0) * u64::from(frames) * faces;
    }
    let mut h: Vec<u8> = Vec::with_capacity(HEADER as usize + len as usize);
    h.extend_from_slice(b"VTF\0");
    h.extend_from_slice(&7u32.to_le_bytes());
    h.extend_from_slice(&5u32.to_le_bytes());
    h.extend_from_slice(&HEADER.to_le_bytes());
    h.extend_from_slice(&(size as u16).to_le_bytes());
    h.extend_from_slice(&(size as u16).to_le_bytes());
    h.extend_from_slice(&flags.to_le_bytes());
    h.extend_from_slice(&(frames as u16).to_le_bytes());
    h.extend_from_slice(&0u16.to_le_bytes());
    h.extend_from_slice(&[0u8; 4]);
    for _ in 0..3 {
        h.extend_from_slice(&1f32.to_le_bytes());
    }
    h.extend_from_slice(&[0u8; 4]);
    h.extend_from_slice(&1f32.to_le_bytes());
    h.extend_from_slice(&format.raw().to_le_bytes());
    h.push(mips as u8);
    h.extend_from_slice(&(-1i32).to_le_bytes());
    h.push(0);
    h.push(0);
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&[0u8; 3]);
    h.extend_from_slice(&1u32.to_le_bytes());
    h.extend_from_slice(&[0u8; 8]);
    h.extend_from_slice(&[0x30, 0, 0, 0]);
    h.extend_from_slice(&HEADER.to_le_bytes());
    debug_assert_eq!(h.len(), HEADER as usize);
    if format.raw() == 15 {
        for m in (0..mips).rev() {
            let side = (size >> m).max(1);
            let idx: u32 = match side {
                1 => 0xffff_fffc,
                2 => 0xffff_f0f0,
                _ => 0,
            };
            let blocks = u64::from(side.div_ceil(4)).pow(2) * u64::from(frames) * faces;
            for _ in 0..blocks {
                h.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0]);
                h.extend_from_slice(&idx.to_le_bytes());
            }
        }
    }
    h.resize(HEADER as usize + len as usize, 0);
    h
}

fn keep_separators(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let sep = c == '/' || c == '\\';
        if sep && (out.is_empty() || out.ends_with(['/', '\\'])) {
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::facts_by_name;

    struct Files(Vec<(&'static str, &'static str)>, Vec<(String, Vec<u8>)>);

    fn sky(name: &str, format: u32, flags: u32) -> Vec<(String, Vec<u8>)> {
        let mut out = Vec::new();
        for face in ["rt", "lf", "bk", "ft", "up", "dn"] {
            let vmt = format!("\"UnlitGeneric\" {{ \"$basetexture\" \"skybox/{name}{face}\" }}");
            out.push((format!("materials/skybox/{name}{face}.vmt"), vmt.into_bytes()));
            let vtf = black_cubemap(hp_vtf::ImageFormat::from_raw(format).unwrap(), flags, 1, 32);
            out.push((format!("materials/skybox/{name}{face}.vtf"), vtf));
        }
        out
    }

    impl MaterialSource for Files {
        fn facts(&self, name: &str) -> MaterialFacts {
            let mut f = facts_by_name(name);
            let path = format!("materials/{}.vmt", hp_vmt::normalize_material_name(name));
            if let Some(text) = self.read_file(&path) {
                let m = hp_vmt::parse(&String::from_utf8_lossy(&text))
                    .unwrap()
                    .resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
                f.envmap = m.get("$envmap").map(str::to_owned);
            }
            f
        }

        fn read_file(&self, path: &str) -> Option<Vec<u8>> {
            self.0
                .iter()
                .find(|(p, _)| p.eq_ignore_ascii_case(path))
                .map(|(_, t)| t.as_bytes().to_vec())
                .or_else(|| self.1.iter().find(|(p, _)| p.eq_ignore_ascii_case(path)).map(|(_, d)| d.clone()))
        }
    }

    fn entity(class: &str, props: &[(&str, &str)]) -> hp_vmf::Entity {
        let mut e = hp_vmf::Entity { classname: class.to_owned(), ..Default::default() };
        for (k, v) in props {
            e.props.push(((*k).to_owned(), (*v).to_owned()));
        }
        e
    }

    fn square(center: DVec3) -> Winding {
        Winding {
            points: vec![
                center + DVec3::new(-8.0, -8.0, 0.0),
                center + DVec3::new(8.0, -8.0, 0.0),
                center + DVec3::new(8.0, 8.0, 0.0),
                center + DVec3::new(-8.0, 8.0, 0.0),
            ],
        }
    }

    #[test]
    fn patches_like_vbsp() {
        let files = Files(
            vec![
                (
                    "materials/concrete/floor.vmt",
                    "\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"concrete/floor\"\n\t\"$envmap\" \"env_cubemap\"\n}\n",
                ),
                ("materials/concrete/plain.vmt", "\"LightmappedGeneric\" { \"$basetexture\" \"concrete/plain\" }"),
                (
                    "materials/glass/dx9.vmt",
                    "\"LightmappedGeneric\" { \"$envmap\" \"env_cubemap\" \"LightmappedGeneric_DX9\" { \"$envmap\" \"env_cubemap\" } \"Proxies\" { \"AnimatedTexture\" { \"animatedtextureframerate\" \"10\" } } }",
                ),
                (
                    "materials/water/sea.vmt",
                    "\"Water\" { \"$bottommaterial\" \"water/sea_beneath\" \"$envmap\" \"env_cubemap\" }",
                ),
                ("materials/water/sea_beneath.vmt", "\"Water\" { \"$envmap\" \"env_cubemap\" }"),
                ("materials/metal/custom.vmt", "\"LightmappedGeneric\" { \"$envmap\" \"metal/shiny\" }"),
            ],
            Vec::new(),
        );
        let world = entity("worldspawn", &[("skyname", "missing")]);
        let a = entity("env_cubemap", &[("origin", "0 0 64.9"), ("cubemapsize", "6")]);
        let b = entity("env_cubemap", &[("origin", "512 0 -64"), ("cubemapsize", "0"), ("sides", "77 78")]);
        let c = entity("env_cubemap", &[("origin", "1024 0 64"), ("sides", "78")]);
        let ents = [&world, &a, &b, &c];
        let mut cm = Cubemaps::new(&ents, Some("GM_Test"), &files);
        assert_eq!(
            cm.samples(),
            &[
                Sample { origin: [0, 0, 64], size: 6 },
                Sample { origin: [512, 0, -64], size: 0 },
                Sample { origin: [1024, 0, 64], size: 0 }
            ]
        );
        let up = DVec3::Z;
        let facts = |n: &str| files.facts(n);
        let w = square(DVec3::new(500.0, 0.0, 0.0));
        let name = cm.side_material("CONCRETE/FLOOR", &facts("CONCRETE/FLOOR"), 5, Some(&w), DVec3::ZERO, up);
        assert_eq!(name.as_deref(), Some("maps/gm_test/concrete/floor_0_0_64"));
        let name =
            cm.side_material("CONCRETE/FLOOR", &facts("CONCRETE/FLOOR"), 6, Some(&w), DVec3::new(500.0, 0.0, 0.0), up);
        assert_eq!(name.as_deref(), Some("maps/gm_test/concrete/floor_1024_0_64"));
        let name =
            cm.side_material("concrete/floor", &facts("concrete/floor"), 7, Some(&w), DVec3::new(0.0, 0.0, 1000.0), up);
        assert_eq!(name.as_deref(), Some("maps/gm_test/concrete/floor_0_0_64"));
        let name = cm.side_material("concrete/floor", &facts("concrete/floor"), 78, Some(&w), DVec3::ZERO, up);
        assert_eq!(name.as_deref(), Some("maps/gm_test/concrete/floor_512_0_-64"));
        assert!(cm.messages.iter().any(|m| m.contains("Multiple references")));
        assert_eq!(cm.side_material("concrete/plain", &facts("concrete/plain"), 8, Some(&w), DVec3::ZERO, up), None);
        assert_eq!(cm.side_material("metal/custom", &facts("metal/custom"), 9, Some(&w), DVec3::ZERO, up), None);
        let name = cm.side_material("glass/dx9", &facts("glass/dx9"), 10, Some(&w), DVec3::ZERO, up).unwrap();
        let water = cm.side_material("water/sea", &facts("water/sea"), 11, Some(&w), DVec3::ZERO, up).unwrap();
        assert_eq!(water, "maps/gm_test/water/sea_0_0_64");
        assert_eq!(cm.dependent_name(&water, "water/sea_beneath"), "maps/gm_test/water/sea_beneath_0_0_64");
        assert_eq!(cm.dependent_name(&water, "water/other"), "water/other");
        assert_eq!(cm.dependent_name("water/unpatched", "water/sea_beneath"), "water/sea_beneath");
        cm.check_sides();
        assert!(cm.messages.iter().any(|m| m.contains("deleted brushside near (512, 0, -64)")));
        let (lump, files_out) = cm.finish(Some("missing"));
        assert_eq!(lump.len(), 48);
        assert_eq!(i32::from_le_bytes(lump[16..20].try_into().unwrap()), 512);
        let get = |p: &str| {
            files_out.iter().find(|(n, _)| n == p).map(|(_, d)| String::from_utf8_lossy(d).replace("\r\n", "\n"))
        };
        assert!(
            files_out.iter().filter(|(n, _)| n.ends_with(".vmt")).all(|(_, d)| d
                .windows(2)
                .filter(|w| *w == b"\r\n")
                .count()
                == d.iter().filter(|b| **b == b'\n').count())
        );
        assert_eq!(
            get("materials/maps/gm_test/concrete/floor_0_0_64.vmt").unwrap(),
            "\"patch\"\n{\n\t\"include\"\t\t\"materials/CONCRETE/FLOOR.vmt\"\n\t\"replace\"\n\t{\n\t\t\"$envmap\"\t\t\"maps/gm_test/c0_0_64\"\n\t}\n}\n"
        );
        let glass = get(&format!("materials/{name}.vmt")).unwrap();
        assert!(
            glass.contains(
                "\t\t\"LightmappedGeneric_DX9\"\n\t\t{\n\t\t\t\"$envmap\"\t\t\"maps/gm_test/c0_0_64\"\n\t\t}\n"
            ),
            "{glass}"
        );
        assert!(glass.contains("\t\t\"Proxies\"\n\t\t{\n\t\t}\n"), "{glass}");
        assert!(!glass.contains("AnimatedTexture"), "{glass}");
        let sea = get("materials/maps/gm_test/water/sea_0_0_64.vmt").unwrap();
        assert!(sea.contains("\"$bottommaterial\"\t\t\"maps/gm_test/water/sea_beneath_0_0_64\""), "{sea}");
        assert!(get("materials/maps/gm_test/water/sea_beneath_0_0_64.vmt").is_some());
        assert!(!files_out.iter().any(|(n, _)| n.ends_with(".vtf")));
        assert_eq!(cm.messages.iter().filter(|m| m.contains("Can't load skybox file skybox/missing")).count(), 1);
    }

    #[test]
    fn default_cubemaps_follow_the_sky() {
        let mut both = sky("sky_day01_09", 3, 0x30c);
        both.extend(sky("sky_day01_01", 3, 0x34c));
        let files = Files(Vec::new(), both);
        let world = entity("worldspawn", &[]);
        let names = |sky: &str| {
            let mut cm = Cubemaps::new(&[&world], Some("gm_sky"), &files);
            let (_, out) = cm.finish(Some(sky));
            out.into_iter().map(|(n, d)| (n, hp_vtf::read_info(&d).unwrap().format.raw(), d.len())).collect::<Vec<_>>()
        };
        assert_eq!(
            names("sky_day01_09"),
            vec![
                ("materials/maps/gm_sky/cubemapdefault.vtf".to_owned(), 3, 24658),
                ("materials/maps/gm_sky/cubemapdefault.hdr.vtf".to_owned(), 24, 65608),
            ]
        );
        assert_eq!(names("sky_day01_01").len(), 2);
        let a = entity("env_cubemap", &[("origin", "0 0 64")]);
        let mut cm = Cubemaps::new(&[&world, &a], Some("gm_sky"), &files);
        let (_, out) = cm.finish(Some("sky_day01_09"));
        let order: Vec<&str> = out.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            order,
            [
                "materials/maps/gm_sky/cubemapdefault.vtf",
                "materials/maps/gm_sky/c0_0_64.vtf",
                "materials/maps/gm_sky/cubemapdefault.hdr.vtf",
                "materials/maps/gm_sky/c0_0_64.hdr.vtf"
            ]
        );
    }

    #[test]
    fn missing_sky_uses_a_real_available_fallback_and_keeps_procedural_skies() {
        let files = Files(Vec::new(), sky("sky_day01_01", 3, 0x34c));
        let world = entity("worldspawn", &[]);
        let mut cm = Cubemaps::new(&[&world], Some("gm_sky"), &files);
        assert_eq!(cm.resolve_sky("nycityday"), "sky_day01_01");
        assert!(cm.messages.iter().any(|line| line.contains("skybox fallback:") && line.contains("nycityday")));
        assert_eq!(cm.resolve_sky("painted"), "painted");
        let missing = Files(Vec::new(), Vec::new());
        let mut cm = Cubemaps::new(&[&world], Some("gm_sky"), &missing);
        assert_eq!(cm.resolve_sky("nycityday"), "nycityday");
    }

    #[test]
    fn hdr_sky_materials_supply_valid_ldr_and_hdr_cubemap_placeholders() {
        let mut source = sky("city_hdr", 3, 0x34c);
        for (name, data) in &mut source {
            if name.ends_with(".vmt") {
                *data = String::from_utf8_lossy(data).replace("$basetexture", "$hdrbasetexture").into_bytes();
            }
        }
        let files = Files(Vec::new(), source);
        let world = entity("worldspawn", &[]);
        let mut cm = Cubemaps::new(&[&world], Some("gm_hdr"), &files);
        assert_eq!(cm.resolve_sky("city"), "city");
        let (_, out) = cm.finish(Some("city"));
        assert_eq!(out.len(), 2);
        assert!(!cm.messages.iter().any(|line| line.contains("Can't load skybox")));
        let hdr = out.iter().find(|(name, _)| name.ends_with(".hdr.vtf")).unwrap();
        let info = hp_vtf::read_info(&hdr.1).unwrap();
        assert_eq!(info.format.raw(), 24);
        assert_eq!(info.flags & SRGB, 0);
        assert_eq!(info.faces, 6);
    }

    #[test]
    fn wvt_patch_values_typed_like_keyvalues() {
        assert_eq!(kv_typed_value(".1"), "0.100000");
        assert_eq!(kv_typed_value("1"), "1");
        assert_eq!(kv_typed_value("01"), "1");
        assert_eq!(kv_typed_value("1.5"), "1.500000");
        assert_eq!(kv_typed_value("-2."), "-2.000000");
        assert_eq!(kv_typed_value("1e2"), "100.000000");
        assert_eq!(kv_typed_value("[1 1 1]"), "[1 1 1]");
        assert_eq!(kv_typed_value("1 2"), "1 2");
        assert_eq!(kv_typed_value("nature/dirt"), "nature/dirt");
        assert_eq!(kv_typed_value(""), "");
    }

    #[test]
    fn placeholder_layout_matches_vbsp() {
        let dxt5 = hp_vtf::ImageFormat::from_raw(15).unwrap();
        let f16 = hp_vtf::ImageFormat::from_raw(24).unwrap();
        let ldr = black_cubemap(dxt5, 0x6300, 1, 32);
        assert_eq!(ldr.len(), 8440);
        assert_eq!(black_cubemap(f16, 0x6300, 1, 32).len(), 65608);
        let info = hp_vtf::read_info(&ldr).unwrap();
        assert_eq!((info.version, info.faces, info.mip_count, info.header_size), ((7, 5), 6, 6, 88));
        assert_eq!(info.flags, 0x6300);
    }

    fn block(id: &mut u32, a: DVec3, b: DVec3, mat: &str) -> String {
        let p = |x: f64, y: f64, z: f64| format!("({x} {y} {z})");
        let planes = [
            format!("{} {} {}", p(a.x, b.y, b.z), p(b.x, b.y, b.z), p(b.x, a.y, b.z)),
            format!("{} {} {}", p(a.x, a.y, a.z), p(b.x, a.y, a.z), p(b.x, b.y, a.z)),
            format!("{} {} {}", p(a.x, b.y, b.z), p(a.x, a.y, b.z), p(a.x, a.y, a.z)),
            format!("{} {} {}", p(b.x, b.y, a.z), p(b.x, a.y, a.z), p(b.x, a.y, b.z)),
            format!("{} {} {}", p(b.x, b.y, b.z), p(a.x, b.y, b.z), p(a.x, b.y, a.z)),
            format!("{} {} {}", p(b.x, a.y, a.z), p(a.x, a.y, a.z), p(a.x, a.y, b.z)),
        ];
        *id += 1;
        let mut s = format!("\tsolid\n\t{{\n\t\t\"id\" \"{id}\"\n");
        for plane in planes {
            *id += 1;
            s += &format!(
                "\t\tside\n\t\t{{\n\t\t\t\"id\" \"{id}\"\n\t\t\t\"plane\" \"{plane}\"\n\t\t\t\"material\" \"{mat}\"\n\t\t\t\"uaxis\" \"[1 0 0 0] 0.25\"\n\t\t\t\"vaxis\" \"[0 -1 0 0] 0.25\"\n\t\t\t\"rotation\" \"0\"\n\t\t\t\"lightmapscale\" \"16\"\n\t\t\t\"smoothing_groups\" \"0\"\n\t\t}}\n"
            );
        }
        s + "\t}\n"
    }

    #[test]
    fn compiled_map_has_cubemaps_and_patches() {
        let files = Files(
            vec![
                (
                    "materials/concrete/floor.vmt",
                    "\"LightmappedGeneric\" { \"$basetexture\" \"concrete/floor\" \"$envmap\" \"env_cubemap\" }",
                ),
                ("materials/dev/wall.vmt", "\"LightmappedGeneric\" { \"$basetexture\" \"dev/wall\" }"),
            ],
            sky("sky_day01_01", 3, 0x34c),
        );
        let mut id = 1;
        let t = 16.0;
        let (lo, hi) = (DVec3::ZERO, DVec3::new(512.0, 256.0, 256.0));
        let mut solids = String::new();
        let boxes = [
            (DVec3::new(lo.x - t, lo.y - t, lo.z - t), DVec3::new(hi.x + t, hi.y + t, lo.z), "CONCRETE/FLOOR"),
            (DVec3::new(lo.x - t, lo.y - t, hi.z), DVec3::new(hi.x + t, hi.y + t, hi.z + t), "dev/wall"),
            (DVec3::new(lo.x - t, lo.y - t, lo.z), DVec3::new(lo.x, hi.y + t, hi.z), "dev/wall"),
            (DVec3::new(hi.x, lo.y - t, lo.z), DVec3::new(hi.x + t, hi.y + t, hi.z), "dev/wall"),
            (DVec3::new(lo.x, lo.y - t, lo.z), DVec3::new(hi.x, lo.y, hi.z), "dev/wall"),
            (DVec3::new(lo.x, hi.y, lo.z), DVec3::new(hi.x, hi.y + t, hi.z), "dev/wall"),
        ];
        for (a, b, m) in boxes {
            solids += &block(&mut id, a, b, m);
        }
        let cube = |id: u32, o: &str| {
            format!(
                "entity\n{{\n\t\"id\" \"{id}\"\n\t\"classname\" \"env_cubemap\"\n\t\"origin\" \"{o}\"\n\t\"cubemapsize\" \"0\"\n}}\n"
            )
        };
        let text = format!(
            "world\n{{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n{solids}}}\n{}{}entity\n{{\n\t\"id\" \"9000\"\n\t\"classname\" \"info_player_start\"\n\t\"origin\" \"32 32 16\"\n}}\n",
            cube(9001, "64 128 64"),
            cube(9002, "448 128 64.5")
        );
        let map = hp_vmf::parse(&text).unwrap();
        let opts = crate::CompileOptions { map_name: Some("gm_cubetest".to_owned()), ..Default::default() };
        let mut log = Vec::new();
        let out =
            crate::compile(&map, &files, &crate::props::NoModels, &opts, &mut |l| log.push(l.to_owned())).unwrap();
        let bsp = hp_bsp::Bsp::from_vec(out.bsp.clone()).unwrap();
        assert_eq!(bsp.cubemaps().unwrap(), vec![([64, 128, 64], 0), ([448, 128, 64], 0)]);
        assert!(bsp.entities().unwrap().iter().all(|e| e.classname() != "env_cubemap"));
        let names = bsp.texture_names().unwrap();
        assert!(names.iter().any(|n| n == "maps/gm_cubetest/concrete/floor_64_128_64"), "{names:?}");
        assert!(!names.iter().any(|n| n == "maps/gm_cubetest/concrete/floor_448_128_64"), "{names:?}");
        assert!(bsp.pakfile().unwrap().contains("materials/maps/gm_cubetest/concrete/floor_448_128_64.vmt"));
        assert!(names.iter().any(|n| n == "dev/wall"));
        let pak = bsp.pakfile().unwrap();
        for f in [
            "materials/maps/gm_cubetest/concrete/floor_64_128_64.vmt",
            "materials/maps/gm_cubetest/c64_128_64.vtf",
            "materials/maps/gm_cubetest/c448_128_64.vtf",
            "materials/maps/gm_cubetest/cubemapdefault.vtf",
        ] {
            assert!(pak.contains(f), "{f}");
        }
        assert!(pak.contains("materials/maps/gm_cubetest/cubemapdefault.hdr.vtf"));
        assert!(crate::validate::validate(&out.bsp).is_empty());
        let mut edit = bsp.edit();
        edit.set_pakfile(hp_bsp::PakWriter::new().finish(hp_bsp::PakOptions::default()).unwrap()).unwrap();
        let problems = crate::validate::validate(&edit.to_vec().unwrap());
        assert_eq!(problems.iter().filter(|p| p.contains("is not in the pakfile")).count(), 1, "{problems:?}");
    }
    fn box_windings(lo: DVec3, hi: DVec3) -> Vec<Option<Vec<glam::Vec3>>> {
        let p = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
        let sides = [
            [p(lo.x, hi.y, hi.z), p(hi.x, hi.y, hi.z), p(hi.x, lo.y, hi.z)],
            [p(lo.x, lo.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, hi.y, lo.z)],
            [p(lo.x, hi.y, hi.z), p(lo.x, lo.y, hi.z), p(lo.x, lo.y, lo.z)],
            [p(hi.x, hi.y, lo.z), p(hi.x, lo.y, lo.z), p(hi.x, lo.y, hi.z)],
            [p(hi.x, hi.y, hi.z), p(lo.x, hi.y, hi.z), p(lo.x, hi.y, lo.z)],
            [p(hi.x, lo.y, lo.z), p(lo.x, lo.y, lo.z), p(lo.x, lo.y, hi.z)],
        ];
        let mut text = String::from(
            "world\n{\n\"id\" \"1\"\n\"classname\" \"worldspawn\"\n}\nentity\n{\n\"id\" \"2\"\n\"classname\" \"parallax_obb\"\n\"targetname\" \"box\"\nsolid\n{\n\"id\" \"3\"\n",
        );
        for (k, s) in sides.iter().enumerate() {
            text.push_str(&format!(
                "side\n{{\n\"id\" \"{}\"\n\"plane\" \"({} {} {}) ({} {} {}) ({} {} {})\"\n\"material\" \"TOOLS/TOOLSSKIP\"\n\"uaxis\" \"[1 0 0 0] 0.25\"\n\"vaxis\" \"[0 -1 0 0] 0.25\"\n\"lightmapscale\" \"16\"\n}}\n",
                k + 4,
                s[0].x, s[0].y, s[0].z, s[1].x, s[1].y, s[1].z, s[2].x, s[2].y, s[2].z
            ));
        }
        text.push_str("}\n}\n");
        let map = hp_vmf::parse(&text).unwrap();
        let ents: Vec<&hp_vmf::Entity> = std::iter::once(&map.world).chain(map.entities.iter()).collect();
        let mut obbs = crate::compile::parallax_obb_windings(&ents, &Files(Vec::new(), Vec::new()));
        assert_eq!(obbs.len(), 1);
        assert_eq!(obbs[0].0, "box");
        obbs.remove(0).1
    }

    fn to_cube(m: &[[f64; 4]; 3], p: DVec3) -> DVec3 {
        let r = |i: usize| m[i][0] * p.x + m[i][1] * p.y + m[i][2] * p.z + m[i][3];
        DVec3::new(r(0), r(1), r(2))
    }

    #[test]
    fn parallax_obb_matrix_maps_the_brush_onto_the_unit_cube() {
        let (lo, hi) = (DVec3::new(-64.0, 32.0, 0.0), DVec3::new(192.0, 160.0, 96.0));
        let m = parallax_obb_matrix(&box_windings(lo, hi));
        for k in 0..8 {
            let c = DVec3::new(
                if k & 1 == 0 { lo.x } else { hi.x },
                if k & 2 == 0 { lo.y } else { hi.y },
                if k & 4 == 0 { lo.z } else { hi.z },
            );
            let q = to_cube(&m, c);
            for a in 0..3 {
                assert!(q[a].abs() < 1e-5 || (q[a] - 1.0).abs() < 1e-5, "corner {c} gives {q}");
            }
        }
        let mid = to_cube(&m, (lo + hi) * 0.5);
        assert!((mid - DVec3::splat(0.5)).length() < 1e-5, "{mid}");
        let rows = parallax_rows(&m);
        assert_eq!(rows.split(';').count(), 3);
        assert!(
            rows.split(';')
                .all(|r| r.starts_with('[') && r.ends_with(']') && r[1..r.len() - 1].split(' ').count() == 4),
            "{rows}"
        );
        let id = parallax_obb_matrix(&[None, Some(vec![glam::Vec3::ZERO; 4])]);
        assert_eq!(
            parallax_rows(&id),
            "[1.000000 0.000000 0.000000 0.000000];[0.000000 1.000000 0.000000 0.000000];[0.000000 0.000000 1.000000 0.000000]"
        );
    }

    #[test]
    fn parallax_patches_insert_the_box() {
        let files = Files(
            vec![
                (
                    "materials/concrete/floor.vmt",
                    "\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"concrete/floor\"\n\t\"$envmap\" \"env_cubemap\"\n}\n",
                ),
                (
                    "materials/water/sea.vmt",
                    "\"Water\" { \"$bottommaterial\" \"water/sea_beneath\" \"$envmap\" \"env_cubemap\" }",
                ),
                ("materials/water/sea_beneath.vmt", "\"Water\" { \"$envmap\" \"env_cubemap\" }"),
            ],
            Vec::new(),
        );
        let world = entity("worldspawn", &[]);
        let a = entity("env_cubemap", &[("origin", "0 0 64"), ("parallaxobb", "room_box")]);
        let b = entity("env_cubemap", &[("origin", "128 0 64"), ("parallaxobb", "ROOM_BOX")]);
        let c = entity("env_cubemap", &[("origin", "1024 0 64"), ("parallaxobb", "missing")]);
        let d = entity("env_cubemap", &[("origin", "2048 0 64")]);
        let ents = [&world, &a, &b, &c, &d];
        let mut cm = Cubemaps::new(&ents, Some("mb_test"), &files);
        let rows = "[0.010000 0.000000 0.000000 -0.500000];[0.000000 0.020000 0.000000 -0.500000];[0.000000 0.000000 0.030000 0.000000]".to_owned();
        cm.resolve_parallax(&[("room_box".to_owned(), rows.clone())]);
        assert_eq!(cm.parallax_of(0), Some(rows.as_str()));
        assert_eq!(cm.parallax_of(1), Some(rows.as_str()), "shared box");
        assert_eq!(cm.parallax_of(2), None);
        assert_eq!(cm.parallax_of(3), None);
        assert!(
            cm.messages.iter().any(|m| m == "Warning: Cannot find parallax obb \"missing\" (num_entities is 1)"),
            "{:?}",
            cm.messages
        );
        let facts = |n: &str| files.facts(n);
        let up = DVec3::Z;
        let at = |x: f64| square(DVec3::new(x, 0.0, 0.0));
        let near_a =
            cm.side_material("concrete/floor", &facts("concrete/floor"), 5, Some(&at(0.0)), DVec3::ZERO, up).unwrap();
        let near_b =
            cm.side_material("concrete/floor", &facts("concrete/floor"), 6, Some(&at(130.0)), DVec3::ZERO, up).unwrap();
        let near_c = cm
            .side_material("concrete/floor", &facts("concrete/floor"), 7, Some(&at(1024.0)), DVec3::ZERO, up)
            .unwrap();
        let near_d = cm
            .side_material("concrete/floor", &facts("concrete/floor"), 8, Some(&at(2048.0)), DVec3::ZERO, up)
            .unwrap();
        let sea = cm.side_material("water/sea", &facts("water/sea"), 9, Some(&at(0.0)), DVec3::ZERO, up).unwrap();
        let (_, out) = cm.finish(None);
        let get = |name: &str| {
            out.iter()
                .find(|(n, _)| *n == format!("materials/{name}.vmt"))
                .map(|(_, d)| String::from_utf8_lossy(d).replace("\r\n", "\n"))
                .unwrap()
        };
        assert_eq!(
            get(&near_a),
            format!(
                "\"patch\"\n{{\n\t\"include\"\t\t\"materials/concrete/floor.vmt\"\n\t\"insert\"\n\t{{\n\t\t\"$envmap\"\t\t\"maps/mb_test/c0_0_64\"\n\t\t\"$envMapParallax\"\t\t\"1\"\n\t\t\"$envMapParallaxOBB1\"\t\t\"[0.010000 0.000000 0.000000 -0.500000]\"\n\t\t\"$envMapParallaxOBB2\"\t\t\"[0.000000 0.020000 0.000000 -0.500000]\"\n\t\t\"$envMapParallaxOBB3\"\t\t\"[0.000000 0.000000 0.030000 0.000000]\"\n\t\t\"$envMapOrigin\"\t\t\"[0 0 64]\"\n\t}}\n}}\n"
            )
        );
        let b_text = get(&near_b);
        assert!(
            b_text.contains("\"$envMapOrigin\"\t\t\"[128 0 64]\"")
                && b_text.contains("[0.010000 0.000000 0.000000 -0.500000]"),
            "{b_text}"
        );
        for name in [&near_c, &near_d] {
            let t = get(name);
            assert!(t.contains("\"replace\"") && !t.contains("Parallax"), "{t}");
        }
        let sea_text = get(&sea);
        assert!(sea_text.contains("\"insert\"") && sea_text.contains("$envMapParallaxOBB3"), "{sea_text}");
        assert!(sea_text.contains("\"$bottommaterial\"\t\t\"maps/mb_test/water/sea_beneath_0_0_64\""), "{sea_text}");
        assert!(get("maps/mb_test/water/sea_beneath_0_0_64").contains("$envMapParallax"));
    }

    #[test]
    fn compiled_map_with_a_parallax_obb() {
        let files = Files(
            vec![
                (
                    "materials/concrete/floor.vmt",
                    "\"LightmappedGeneric\" { \"$basetexture\" \"concrete/floor\" \"$envmap\" \"env_cubemap\" }",
                ),
                ("materials/dev/wall.vmt", "\"LightmappedGeneric\" { \"$basetexture\" \"dev/wall\" }"),
            ],
            sky("sky_day01_01", 3, 0x34c),
        );
        let room = |with_obb: bool| {
            let mut id = 1;
            let t = 16.0;
            let (lo, hi) = (DVec3::ZERO, DVec3::new(512.0, 256.0, 256.0));
            let mut solids = String::new();
            let boxes = [
                (DVec3::new(lo.x - t, lo.y - t, lo.z - t), DVec3::new(hi.x + t, hi.y + t, lo.z), "concrete/floor"),
                (DVec3::new(lo.x - t, lo.y - t, hi.z), DVec3::new(hi.x + t, hi.y + t, hi.z + t), "dev/wall"),
                (DVec3::new(lo.x - t, lo.y - t, lo.z), DVec3::new(lo.x, hi.y + t, hi.z), "dev/wall"),
                (DVec3::new(hi.x, lo.y - t, lo.z), DVec3::new(hi.x + t, hi.y + t, hi.z), "dev/wall"),
                (DVec3::new(lo.x, lo.y - t, lo.z), DVec3::new(hi.x, lo.y, hi.z), "dev/wall"),
                (DVec3::new(lo.x, hi.y, lo.z), DVec3::new(hi.x, hi.y + t, hi.z), "dev/wall"),
            ];
            for (a, b, m) in boxes {
                solids += &block(&mut id, a, b, m);
            }
            let obb = if with_obb {
                let mut id = 5000;
                format!(
                    "entity\n{{\n\t\"id\" \"4999\"\n\t\"classname\" \"parallax_obb\"\n\t\"targetname\" \"room\"\n{}}}\n",
                    block(&mut id, lo, hi, "tools/toolsskip")
                )
            } else {
                String::new()
            };
            let key = if with_obb { "\t\"parallaxobb\" \"room\"\n" } else { "" };
            format!(
                "world\n{{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n{solids}}}\n{obb}entity\n{{\n\t\"id\" \"9001\"\n\t\"classname\" \"env_cubemap\"\n\t\"origin\" \"256 128 64\"\n{key}}}\nentity\n{{\n\t\"id\" \"9000\"\n\t\"classname\" \"info_player_start\"\n\t\"origin\" \"32 32 16\"\n}}\n"
            )
        };
        let opts = crate::CompileOptions { map_name: Some("mb_box".to_owned()), ..Default::default() };
        let compile = |text: &str| {
            crate::compile(&hp_vmf::parse(text).unwrap(), &files, &crate::props::NoModels, &opts, &mut |_| {}).unwrap()
        };
        let plain = compile(&room(false));
        let out = compile(&room(true));
        let bsp = hp_bsp::Bsp::from_vec(out.bsp.clone()).unwrap();
        assert!(bsp.entities().unwrap().iter().all(|e| e.classname() != "parallax_obb"));
        let plain_bsp = hp_bsp::Bsp::from_vec(plain.bsp.clone()).unwrap();
        for l in [hp_bsp::lump::MODELS, 18, 19] {
            assert_eq!(bsp.lump(l).unwrap(), plain_bsp.lump(l).unwrap(), "lump {l}");
        }
        let pak = bsp.pakfile().unwrap();
        let vmt = String::from_utf8_lossy(
            &pak.read_name("materials/maps/mb_box/concrete/floor_256_128_64.vmt").unwrap().unwrap(),
        )
        .into_owned();
        assert!(
            vmt.contains("\"insert\"")
                && vmt.contains("\"$envMapParallax\"\t\t\"1\"")
                && vmt.contains("\"$envMapOrigin\"\t\t\"[256 128 64]\""),
            "{vmt}"
        );
        let row = |k: usize| -> [f64; 4] {
            let key = format!("\"$envMapParallaxOBB{k}\"\t\t\"[");
            let start = vmt.find(&key).unwrap() + key.len();
            let end = start + vmt[start..].find(']').unwrap();
            let v: Vec<f64> = vmt[start..end].split(' ').map(|x| x.parse().unwrap()).collect();
            [v[0], v[1], v[2], v[3]]
        };
        let m = [row(1), row(2), row(3)];
        let center = to_cube(&m, DVec3::new(256.0, 128.0, 128.0));
        assert!((center - DVec3::splat(0.5)).length() < 1e-4, "{center}");
        assert!(crate::validate::validate(&out.bsp).is_empty());
    }
}
