use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use hp_app::state::MaterialInfo;
use hp_fs::{AssetEntry, MaterialKeywords};

pub use hp_app::source_filter::{SourceFilter, SourceGroup, SourceMeta, SourceTable};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MatFlag {
    Translucent,
    Alpha,
    SelfIllum,
    EnvMap,
    Blend,
    Missing,
}

impl MatFlag {
    pub const ALL: [MatFlag; 6] =
        [MatFlag::Translucent, MatFlag::Alpha, MatFlag::SelfIllum, MatFlag::EnvMap, MatFlag::Blend, MatFlag::Missing];

    pub fn word(self) -> &'static str {
        match self {
            MatFlag::Translucent => "translucent",
            MatFlag::Alpha => "alpha",
            MatFlag::SelfIllum => "selfillum",
            MatFlag::EnvMap => "envmap",
            MatFlag::Blend => "blend",
            MatFlag::Missing => "missing",
        }
    }

    pub fn of_word(word: &str) -> Option<MatFlag> {
        MatFlag::ALL.into_iter().find(|f| f.word().eq_ignore_ascii_case(word))
    }

    pub fn bit(self) -> u8 {
        1 << (self as u8)
    }

    pub fn of_info(self, info: &MaterialInfo) -> bool {
        flag_bits_of_info(info) & self.bit() != 0
    }
}

pub type FlagBits = u8;

pub const FLAGS_KNOWN: FlagBits = 0x80;

pub fn flag_bits_of_info(info: &MaterialInfo) -> FlagBits {
    let mut b = FLAGS_KNOWN;
    if info.translucent || info.additive {
        b |= MatFlag::Translucent.bit();
    }
    if info.shows_alpha {
        b |= MatFlag::Alpha.bit();
    }
    if info.self_illum {
        b |= MatFlag::SelfIllum.bit();
    }
    if info.env_map {
        b |= MatFlag::EnvMap.bit();
    }
    if info.is_blend {
        b |= MatFlag::Blend.bit();
    }
    if info.missing {
        b |= MatFlag::Missing.bit();
    }
    b
}

pub fn flag_bits_of_material(mat: Option<&hp_vmt::Material>, texture: impl Fn(&str) -> Option<bool>) -> FlagBits {
    let Some(m) = mat else { return FLAGS_KNOWN | MatFlag::Missing.bit() };
    let mut b = FLAGS_KNOWN;
    if m.translucent() || m.additive() {
        b |= MatFlag::Translucent.bit();
    }
    let resolved = m.resolve_fallbacks(hp_vmt::DEFAULT_DX_LEVEL, false);
    let usage = resolved.alpha_usage(&|t| texture(t).unwrap_or(false));
    if usage.is_see_through() && !resolved.is_tool() && !resolved.is_water() {
        b |= MatFlag::Alpha.bit();
    }
    if m.self_illum() {
        b |= MatFlag::SelfIllum.bit();
    }
    if m.env_map().is_some() {
        b |= MatFlag::EnvMap.bit();
    }
    if m.is_blend() {
        b |= MatFlag::Blend.bit();
    }
    if m.tool_texture().is_none()
        && !m.is_water()
        && let Some(t) = m.base_texture()
        && texture(&t).is_none()
    {
        b |= MatFlag::Missing.bit();
    }
    b
}

#[derive(Clone, Debug, Default)]
pub struct MaterialFlagsIndex {
    bits: Vec<FlagBits>,
    shaders: Vec<u16>,
    shader_names: Vec<String>,
}

impl MaterialFlagsIndex {
    pub fn build(fs: &hp_fs::GameFs, list: &[AssetEntry], threads: usize) -> MaterialFlagsIndex {
        let threads = threads.clamp(1, 16);
        let chunk = list.len().div_ceil(threads).max(1);
        let mut bits = vec![0u8; list.len()];
        let mut shader_of: Vec<String> = vec![String::new(); list.len()];
        std::thread::scope(|scope| {
            for ((entries, out), shaders) in
                list.chunks(chunk).zip(bits.chunks_mut(chunk)).zip(shader_of.chunks_mut(chunk))
            {
                scope.spawn(move || {
                    hp_sched::lower_current_thread(hp_sched::Level::BelowNormal);
                    for ((e, slot), shader) in entries.iter().zip(out.iter_mut()).zip(shaders.iter_mut()) {
                        let mat = fs.load_material(&e.name);
                        *slot = flag_bits_of_material(mat.as_ref(), |t| fs.texture_alpha_flag(t));
                        if let Some(m) = &mat {
                            *shader = hp_vmt::shader_base_name(&m.shader);
                        }
                    }
                });
            }
        });
        let mut shader_names: Vec<String> = Vec::new();
        let shaders = shader_of
            .into_iter()
            .map(|name| {
                if name.is_empty() {
                    return u16::MAX;
                }
                match shader_names.iter().position(|n| *n == name) {
                    Some(i) => i as u16,
                    None if shader_names.len() < usize::from(u16::MAX) => {
                        shader_names.push(name);
                        (shader_names.len() - 1) as u16
                    }
                    None => u16::MAX,
                }
            })
            .collect();
        MaterialFlagsIndex { bits, shaders, shader_names }
    }

    pub fn shader(&self, i: usize) -> Option<&str> {
        let s = *self.shaders.get(i)?;
        self.shader_names.get(usize::from(s)).map(String::as_str)
    }

    pub fn shader_counts(&self) -> Vec<(String, usize)> {
        let mut counts = vec![0usize; self.shader_names.len()];
        for &s in &self.shaders {
            if let Some(c) = counts.get_mut(usize::from(s)) {
                *c += 1;
            }
        }
        let mut out: Vec<(String, usize)> = self.shader_names.iter().cloned().zip(counts).collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        out
    }

    pub fn get(&self, i: usize) -> FlagBits {
        self.bits.get(i).copied().unwrap_or(0)
    }

    pub fn len(&self) -> usize {
        self.bits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bits.is_empty()
    }
}

fn fold_char(c: char, out: &mut String) {
    match c {
        'A'..='Z' => out.push(c.to_ascii_lowercase()),
        '\\' => out.push('/'),
        c if c.is_ascii() => out.push(c),
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' => out.push('a'),
        'æ' | 'Æ' => out.push_str("ae"),
        'ç' | 'Ç' => out.push('c'),
        'è' | 'é' | 'ê' | 'ë' | 'È' | 'É' | 'Ê' | 'Ë' => out.push('e'),
        'ì' | 'í' | 'î' | 'ï' | 'Ì' | 'Í' | 'Î' | 'Ï' => out.push('i'),
        'ñ' | 'Ñ' => out.push('n'),
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' => out.push('o'),
        'œ' | 'Œ' => out.push_str("oe"),
        'ù' | 'ú' | 'û' | 'ü' | 'Ù' | 'Ú' | 'Û' | 'Ü' => out.push('u'),
        'ý' | 'ÿ' | 'Ý' => out.push('y'),
        'ß' => out.push_str("ss"),
        c => out.extend(c.to_lowercase()),
    }
}

pub fn fold_into(s: &str, out: &mut String) {
    for c in s.chars() {
        fold_char(c, out);
    }
}

pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    fold_into(s, &mut out);
    out
}

const PATH_EXTENSIONS: &[&str] = &[".vmt", ".vtf", ".png", ".jpg", ".jpeg", ".tga", ".bmp"];

pub fn material_term(word: &str) -> &str {
    let mut t = word.trim_matches(['"', '\'']);
    if let Some(i) = t.rmatch_indices("materials/").map(|(i, _)| i).find(|&i| i == 0 || t[..i].ends_with('/')) {
        t = &t[i + "materials/".len()..];
    }
    for ext in PATH_EXTENSIONS {
        if let Some(stem) = t.strip_suffix(ext)
            && !stem.is_empty()
        {
            t = stem;
            break;
        }
    }
    let trimmed = t.trim_matches('/');
    if trimmed.is_empty() { t } else { trimmed }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub terms: Vec<String>,
    pub excluded: Vec<String>,
    pub keywords: Vec<String>,
    pub sources: Vec<String>,
    pub flags: Vec<MatFlag>,
    pub not_flags: Vec<MatFlag>,
    pub shaders: Vec<String>,
    pub not_shaders: Vec<String>,
}

impl Query {
    pub fn parse(text: &str) -> Query {
        let mut q = Query::default();
        for word in text.split_whitespace() {
            let folded = fold(word);
            if let Some(rest) = folded.strip_prefix("kw:") {
                if !rest.is_empty() {
                    q.keywords.push(rest.to_owned());
                }
            } else if let Some(rest) = folded.strip_prefix("src:") {
                if !rest.is_empty() {
                    q.sources.push(rest.to_owned());
                }
            } else if let Some(rest) = folded.strip_prefix("is:") {
                if let Some(f) = MatFlag::of_word(rest)
                    && !q.flags.contains(&f)
                {
                    q.flags.push(f);
                }
            } else if let Some(rest) = folded.strip_prefix("shader:") {
                if !rest.is_empty() {
                    q.shaders.push(rest.to_owned());
                }
            } else if let Some(rest) = folded.strip_prefix("-shader:") {
                if !rest.is_empty() {
                    q.not_shaders.push(rest.to_owned());
                }
            } else if let Some(rest) = folded.strip_prefix("-is:") {
                if let Some(f) = MatFlag::of_word(rest)
                    && !q.not_flags.contains(&f)
                {
                    q.not_flags.push(f);
                }
            } else if let Some(rest) = folded.strip_prefix('-') {
                let rest = material_term(rest);
                if !rest.is_empty() {
                    q.excluded.push(rest.to_owned());
                }
            } else {
                let term = material_term(&folded);
                if !term.is_empty() {
                    q.terms.push(term.to_owned());
                }
            }
        }
        q
    }

    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
            && self.excluded.is_empty()
            && self.keywords.is_empty()
            && self.sources.is_empty()
            && self.flags.is_empty()
            && self.not_flags.is_empty()
            && self.shaders.is_empty()
            && self.not_shaders.is_empty()
    }

    pub fn needs_info(&self) -> bool {
        !self.keywords.is_empty()
    }

    pub fn needs_flags(&self) -> bool {
        !self.flags.is_empty() || !self.not_flags.is_empty() || self.needs_shader()
    }

    pub fn needs_shader(&self) -> bool {
        !self.shaders.is_empty() || !self.not_shaders.is_empty()
    }
}

pub fn toggle_flag_word(text: &str, flag: MatFlag) -> String {
    let token = format!("is:{}", flag.word());
    let mut words: Vec<&str> = text.split_whitespace().collect();
    let before = words.len();
    words.retain(|w| !w.eq_ignore_ascii_case(&token));
    if words.len() == before {
        words.push(&token);
        return words.join(" ");
    }
    words.join(" ")
}

pub fn is_excluded(name: &str, excluded: &[String]) -> bool {
    excluded.iter().any(|dir| {
        let dir = dir.trim().trim_matches('/');
        !dir.is_empty()
            && name.len() > dir.len()
            && name.as_bytes()[dir.len()] == b'/'
            && name[..dir.len()].eq_ignore_ascii_case(dir)
    })
}

pub fn normalize_exclusions<'a>(dirs: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for d in dirs {
        let mut n = d.trim().replace('\\', "/").to_ascii_lowercase();
        if let Some(rest) = n.strip_prefix("materials/") {
            n = rest.to_owned();
        }
        let n = n.trim_matches('/').to_owned();
        if !n.is_empty() && !out.contains(&n) {
            out.push(n);
        }
    }
    out
}

#[derive(Clone, Debug, Default)]
pub struct SearchIndex {
    text: String,
    ends: Vec<u32>,
}

impl SearchIndex {
    pub fn build(entries: &[AssetEntry]) -> SearchIndex {
        let total: usize = entries.iter().map(|e| e.name.len()).sum();
        let mut text = String::with_capacity(total);
        let mut ends = Vec::with_capacity(entries.len());
        for e in entries {
            fold_into(&e.name, &mut text);
            ends.push(u32::try_from(text.len()).unwrap_or(u32::MAX));
        }
        SearchIndex { text, ends }
    }

    pub fn key(&self, i: usize) -> &str {
        let start = if i == 0 { 0 } else { self.ends.get(i - 1).copied().unwrap_or(0) as usize };
        let end = self.ends.get(i).copied().unwrap_or(0) as usize;
        self.text.get(start..end).unwrap_or("")
    }

    pub fn len(&self) -> usize {
        self.ends.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Category {
    #[default]
    All,
    Used,
    Favorites,
    Recent,
    Tools,
}

pub const MISSING_SOURCE: u32 = u32::MAX;

pub struct FilterParams<'a> {
    pub query: &'a Query,
    pub source: SourceFilter,
    pub category: Category,
    pub hide_tools: bool,
    pub members: &'a [String],
    pub infos: &'a HashMap<String, Arc<MaterialInfo>>,
    pub keywords: Option<&'a MaterialKeywords>,
    pub sources: &'a SourceTable,
    pub flags: Option<&'a MaterialFlagsIndex>,
    pub excluded: &'a [String],
    pub hidden_shaders: &'a [String],
}

#[derive(Clone, Debug, Default)]
pub struct Filtered {
    pub indices: Vec<u32>,
    pub extras: Vec<AssetEntry>,
}

impl Filtered {
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn get<'a>(&'a self, list: &'a [AssetEntry], i: usize) -> Option<&'a AssetEntry> {
        let idx = *self.indices.get(i)? as usize;
        if idx < list.len() { list.get(idx) } else { self.extras.get(idx - list.len()) }
    }

    pub fn position(&self, list: &[AssetEntry], name: &str) -> Option<usize> {
        (0..self.len()).find(|&i| self.get(list, i).is_some_and(|e| e.name == name))
    }
}

pub fn find_in_list(list: &[AssetEntry], name: &str) -> Option<usize> {
    list.binary_search_by(|e| e.name.as_str().cmp(name)).ok()
}

fn name_matches(key: &str, q: &Query) -> bool {
    q.terms.iter().all(|t| key.contains(t.as_str())) && !q.excluded.iter().any(|t| key.contains(t.as_str()))
}

fn keywords_match(entry: &AssetEntry, q: &Query, p: &FilterParams<'_>) -> bool {
    if q.keywords.is_empty() {
        return true;
    }
    let name = entry.name.as_str();
    let mut buf = String::new();
    let mut matches = |kw: &str, k: &str| {
        buf.clear();
        fold_into(kw, &mut buf);
        buf.contains(k)
    };
    if let Some(index) = p.keywords.filter(|i| i.covers(entry.source)) {
        return q.keywords.iter().all(|k| index.get(name).any(|kw| matches(kw, k)));
    }
    let Some(info) = p.infos.get(name) else { return false };
    q.keywords.iter().all(|k| info.keywords.iter().any(|kw| matches(kw, k)))
}

fn source_label_matches(source: u32, q: &Query, sources: &SourceTable) -> bool {
    if q.sources.is_empty() {
        return true;
    }
    let Some(meta) = sources.get(source) else { return false };
    q.sources.iter().all(|s| meta.folded_label.contains(s.as_str()))
}

fn flags_of(entry: &AssetEntry, index: Option<usize>, info: Option<&MaterialInfo>, p: &FilterParams<'_>) -> FlagBits {
    if entry.source == MISSING_SOURCE {
        return FLAGS_KNOWN | MatFlag::Missing.bit();
    }
    let indexed = index.and_then(|i| p.flags.map(|f| f.get(i))).filter(|b| b & FLAGS_KNOWN != 0);
    indexed.or_else(|| info.map(flag_bits_of_info)).unwrap_or(0)
}

fn flags_match(bits: FlagBits, q: &Query) -> bool {
    if bits & FLAGS_KNOWN == 0 {
        return q.flags.is_empty() && q.not_flags.is_empty();
    }
    q.flags.iter().all(|f| bits & f.bit() != 0) && !q.not_flags.iter().any(|f| bits & f.bit() != 0)
}

fn passes(entry: &AssetEntry, key: &str, index: Option<usize>, p: &FilterParams<'_>) -> bool {
    if !name_matches(key, p.query) {
        return false;
    }
    let info = p.infos.get(&entry.name).map(Arc::as_ref);
    let tool = crate::util::is_tool_material(&entry.name, info);
    if p.hide_tools && tool && p.category != Category::Tools {
        return false;
    }
    if p.category == Category::Tools && !tool {
        return false;
    }
    if !tool && p.category != Category::Used && is_excluded(&entry.name, p.excluded) {
        return false;
    }
    if p.query.needs_flags() && !flags_match(flags_of(entry, index, info, p), p.query) {
        return false;
    }
    if p.query.needs_shader() || (!p.hidden_shaders.is_empty() && !tool) {
        let indexed = index.and_then(|i| p.flags.and_then(|f| f.shader(i)));
        let shader = indexed.map(str::to_owned).or_else(|| info.map(|i| hp_vmt::shader_base_name(&i.shader)));
        match shader.as_deref() {
            Some(s) => {
                if !p.query.shaders.iter().all(|w| s.contains(w.as_str()))
                    || p.query.not_shaders.iter().any(|w| s.contains(w.as_str()))
                    || (!tool && p.hidden_shaders.iter().any(|h| s == h.as_str()))
                {
                    return false;
                }
            }
            None if !p.query.shaders.is_empty() => return false,
            None => {}
        }
    }
    if entry.source == MISSING_SOURCE {
        if p.source != SourceFilter::All || !p.query.sources.is_empty() {
            return false;
        }
    } else if !p.sources.allows(p.source, entry.source) || !source_label_matches(entry.source, p.query, p.sources) {
        return false;
    }
    keywords_match(entry, p.query, p)
}

pub fn filter_materials(list: &[AssetEntry], index: &SearchIndex, p: &FilterParams<'_>) -> Filtered {
    let mut out = Filtered::default();
    match p.category {
        Category::All | Category::Tools => {
            out.indices.reserve(if p.query.is_empty() { list.len() } else { 256 });
            for (i, entry) in list.iter().enumerate() {
                if passes(entry, index.key(i), Some(i), p) {
                    out.indices.push(u32::try_from(i).unwrap_or(u32::MAX));
                }
            }
        }
        Category::Used | Category::Favorites | Category::Recent => {
            let mut seen: HashSet<&str> = HashSet::with_capacity(p.members.len());
            let mut key = String::new();
            for name in p.members {
                if !seen.insert(name.as_str()) {
                    continue;
                }
                match find_in_list(list, name) {
                    Some(i) => {
                        if passes(&list[i], index.key(i), Some(i), p) {
                            out.indices.push(u32::try_from(i).unwrap_or(u32::MAX));
                        }
                    }
                    None => {
                        let entry = AssetEntry { name: name.clone(), source: MISSING_SOURCE };
                        key.clear();
                        fold_into(name, &mut key);
                        if passes(&entry, &key, None, p) {
                            let idx = list.len() + out.extras.len();
                            out.indices.push(u32::try_from(idx).unwrap_or(u32::MAX));
                            out.extras.push(entry);
                        }
                    }
                }
            }
        }
    }
    out
}

pub fn used_materials(doc: &hp_editor::Document) -> Vec<String> {
    let mut set: HashSet<String> = HashSet::new();
    let mut buf = String::new();
    for node in doc.scene().solids().values() {
        for side in &node.solid.sides {
            crate::util::normalize_material_into(&side.material, &mut buf);
            if !buf.is_empty() && !set.contains(buf.as_str()) {
                set.insert(buf.clone());
            }
        }
    }
    let mut v: Vec<String> = set.into_iter().collect();
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(name: &str, source: u32) -> AssetEntry {
        AssetEntry { name: name.to_owned(), source }
    }

    fn list() -> Vec<AssetEntry> {
        let mut v = vec![
            e("concrete/concretefloor001a", 1),
            e("concrete/concretewall004b", 1),
            e("brick/brickwall001a", 2),
            e("custom/textures_ete/mur_brique", 10),
            e("tools/toolsnodraw", 1),
            e("tools/toolsclip", 1),
            e("metal/metalfloor001a", 3),
            e("wood/woodfloor005a", 4),
        ];
        v.sort();
        v
    }

    fn sources() -> SourceTable {
        let mut t = SourceTable::default();
        t.insert(1, Some(SourceGroup::Game), "garrysmod.vpk");
        t.insert(2, Some(SourceGroup::Addons), "Addon: Bricks Pack");
        t.insert(3, Some(SourceGroup::Workshop), "Workshop: Metal Textures");
        t.insert(4, Some(SourceGroup::Game), "hl2_textures_dir.vpk");
        t.insert(10, Some(SourceGroup::Custom), "Custom: Textures Été");
        t
    }

    fn run(list: &[AssetEntry], text: &str, f: impl FnOnce(&mut FilterParams<'_>)) -> Vec<String> {
        run_with(list, text, &[], f)
    }

    fn run_with(
        list: &[AssetEntry],
        text: &str,
        members: &[String],
        f: impl FnOnce(&mut FilterParams<'_>),
    ) -> Vec<String> {
        let q = Query::parse(text);
        let infos = HashMap::new();
        let src = sources();
        let index = SearchIndex::build(list);
        let mut p = FilterParams {
            query: &q,
            source: SourceFilter::All,
            category: Category::All,
            hide_tools: false,
            members,
            infos: &infos,
            keywords: None,
            sources: &src,
            flags: None,
            excluded: &[],
            hidden_shaders: &[],
        };
        f(&mut p);
        let r = filter_materials(list, &index, &p);
        (0..r.len()).filter_map(|i| r.get(list, i)).map(|e| e.name.clone()).collect()
    }

    #[test]
    fn folding() {
        assert_eq!(fold("Mur_Brique ÉTÉ"), "mur_brique ete");
        assert_eq!(fold("Cœur\\Œuvre"), "coeur/oeuvre");
        assert_eq!(fold("ÀÇÑ"), "acn");
    }

    #[test]
    fn query_parsing() {
        let q = Query::parse("  Floor  kw:Metal src:Work -wall kw: - ");
        assert_eq!(q.terms, ["floor"]);
        assert_eq!(q.keywords, ["metal"]);
        assert_eq!(q.sources, ["work"]);
        assert_eq!(q.excluded, ["wall"]);
        assert!(q.needs_info());
        assert!(Query::parse("   ").is_empty());
    }

    #[test]
    fn pasted_material_paths() {
        assert_eq!(material_term("materials/brick/brickwall001a.vmt"), "brick/brickwall001a");
        assert_eq!(material_term("c:/steam/garrysmod/materials/brick/a.vtf"), "brick/a");
        assert_eq!(material_term("\"materials/custom/x.png\""), "custom/x");
        assert_eq!(material_term("/brick/"), "brick");
        assert_eq!(material_term("mymaterials/x"), "mymaterials/x");
        assert_eq!(material_term(".vmt"), ".vmt");
        assert_eq!(material_term("floor"), "floor");
        let q = Query::parse("MATERIALS\\Brick\\BrickWall001A.vmt -Materials/Dev/X.VMT");
        assert_eq!(q.terms, ["brick/brickwall001a"]);
        assert_eq!(q.excluded, ["dev/x"]);
        let l = list();
        assert_eq!(run(&l, "MATERIALS\\Brick\\BrickWall001A.vmt", |_| {}), ["brick/brickwall001a"]);
        assert_eq!(run(&l, "\"materials/tools/toolsnodraw.vtf\"", |_| {}), ["tools/toolsnodraw"]);
    }

    #[test]
    fn keyword_index_is_used_when_available() {
        let dir = crate::tests::scratch("kw_index");
        let write = |rel: &str, text: &str| {
            let path = dir.join(rel);
            let _ = std::fs::create_dir_all(path.parent().unwrap_or(&dir));
            let _ = std::fs::write(path, text);
        };
        write("materials/concrete/concretewall004b.vmt", "LightmappedGeneric { %keywords \"Urban, Béton\" }");
        write("materials/concrete/concretefloor001a.vmt", "LightmappedGeneric { $basetexture x }");
        let fs = hp_fs::GameFs::empty();
        let _ = fs.add_game_dir(&dir);
        let keywords = fs.material_keywords();
        let l = list();
        let infos = HashMap::new();
        let src = sources();
        let index = SearchIndex::build(&l);
        let names = |text: &str, keywords: Option<&MaterialKeywords>| {
            let q = Query::parse(text);
            let p = FilterParams {
                query: &q,
                source: SourceFilter::All,
                category: Category::All,
                hide_tools: false,
                members: &[],
                infos: &infos,
                keywords,
                sources: &src,
                flags: None,
                excluded: &[],
                hidden_shaders: &[],
            };
            let r = filter_materials(&l, &index, &p);
            (0..r.len()).filter_map(|i| r.get(&l, i)).map(|e| e.name.clone()).collect::<Vec<_>>()
        };
        assert_eq!(names("kw:urb", Some(&keywords)), ["concrete/concretewall004b"]);
        assert_eq!(names("kw:BETON concrete", Some(&keywords)), ["concrete/concretewall004b"]);
        assert!(names("kw:urban floor", Some(&keywords)).is_empty());
        assert!(names("kw:urb", None).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multi_word_and() {
        let l = list();
        assert_eq!(run(&l, "floor concrete", |_| {}), ["concrete/concretefloor001a"]);
        assert_eq!(run(&l, "FLOOR", |_| {}).len(), 3);
        assert_eq!(run(&l, "floor -metal -wood", |_| {}), ["concrete/concretefloor001a"]);
        assert_eq!(run(&l, "", |_| {}).len(), l.len());
        assert!(run(&l, "nothing_like_this", |_| {}).is_empty());
    }

    #[test]
    fn accents_insensitive() {
        let l = list();
        assert_eq!(run(&l, "TEXTURES_ÉTÉ", |_| {}), ["custom/textures_ete/mur_brique"]);
        assert_eq!(run(&l, "ÉTÉ", |_| {}).len(), 3);
        assert_eq!(run(&l, "brique", |_| {}), ["custom/textures_ete/mur_brique"]);
    }

    #[test]
    fn source_label_filter() {
        let l = list();
        assert_eq!(run(&l, "src:workshop", |_| {}), ["metal/metalfloor001a"]);
        assert_eq!(run(&l, "src:été", |_| {}), ["custom/textures_ete/mur_brique"]);
        assert_eq!(run(&l, "src:bricks wall", |_| {}), ["brick/brickwall001a"]);
    }

    #[test]
    fn source_chips() {
        let l = list();
        assert_eq!(run(&l, "", |p| p.source = SourceFilter::Addons), ["brick/brickwall001a"]);
        assert_eq!(run(&l, "", |p| p.source = SourceFilter::Workshop), ["metal/metalfloor001a"]);
        assert_eq!(run(&l, "", |p| p.source = SourceFilter::Source(10)), ["custom/textures_ete/mur_brique"]);
        assert_eq!(run(&l, "", |p| p.source = SourceFilter::Game).len(), 5);
    }

    #[test]
    fn keywords_need_loaded_info() {
        let l = list();
        let q = Query::parse("kw:urban");
        let mut infos = HashMap::new();
        infos.insert(
            "concrete/concretewall004b".to_owned(),
            Arc::new(MaterialInfo { keywords: vec!["Urban".into(), "concrete".into()], ..Default::default() }),
        );
        infos.insert("concrete/concretefloor001a".to_owned(), Arc::new(MaterialInfo::default()));
        let src = sources();
        let index = SearchIndex::build(&l);
        let p = FilterParams {
            query: &q,
            source: SourceFilter::All,
            category: Category::All,
            hide_tools: false,
            members: &[],
            infos: &infos,
            keywords: None,
            sources: &src,
            flags: None,
            excluded: &[],
            hidden_shaders: &[],
        };
        let r = filter_materials(&l, &index, &p);
        assert_eq!(r.len(), 1);
        assert_eq!(r.get(&l, 0).map(|e| e.name.as_str()), Some("concrete/concretewall004b"));
    }

    #[test]
    fn hide_tools_and_tools_category() {
        let l = list();
        let all = run(&l, "", |p| p.hide_tools = true);
        assert_eq!(all.len(), l.len() - 2);
        assert!(all.iter().all(|n| !n.starts_with("tools/")));
        let tools = run(&l, "", |p| {
            p.category = Category::Tools;
            p.hide_tools = true;
        });
        assert_eq!(tools, ["tools/toolsclip", "tools/toolsnodraw"]);
    }

    #[test]
    fn member_categories() {
        let l = list();
        let members = vec![
            "wood/woodfloor005a".to_owned(),
            "missing/texture".to_owned(),
            "concrete/concretefloor001a".to_owned(),
            "wood/woodfloor005a".to_owned(),
        ];
        let recent = run_with(&l, "", &members, |p| {
            p.category = Category::Recent;
        });
        assert_eq!(recent, ["wood/woodfloor005a", "missing/texture", "concrete/concretefloor001a"]);
        let used = run_with(&l, "floor", &members, |p| {
            p.category = Category::Used;
        });
        assert_eq!(used, ["wood/woodfloor005a", "concrete/concretefloor001a"]);
        let favs = run_with(&l, "", &members, |p| {
            p.category = Category::Favorites;
            p.source = SourceFilter::Game;
        });
        assert_eq!(favs, ["wood/woodfloor005a", "concrete/concretefloor001a"]);
    }

    #[test]
    fn flag_queries_and_chips() {
        let q = Query::parse("glass is:Translucent -is:envmap is:nothing");
        assert_eq!(q.terms, ["glass"]);
        assert_eq!(q.flags, [MatFlag::Translucent]);
        assert_eq!(q.not_flags, [MatFlag::EnvMap]);
        assert!(q.needs_flags());
        assert_eq!(toggle_flag_word("glass", MatFlag::Alpha), "glass is:alpha");
        assert_eq!(toggle_flag_word("glass IS:ALPHA", MatFlag::Alpha), "glass");
        let l = list();
        let mut infos = HashMap::new();
        infos.insert(
            "brick/brickwall001a".to_owned(),
            Arc::new(MaterialInfo { translucent: true, ..Default::default() }),
        );
        infos.insert(
            "wood/woodfloor005a".to_owned(),
            Arc::new(MaterialInfo { env_map: true, translucent: true, ..Default::default() }),
        );
        let src = sources();
        let index = SearchIndex::build(&l);
        let names = |text: &str, flags: Option<&MaterialFlagsIndex>| {
            let q = Query::parse(text);
            let p = FilterParams {
                query: &q,
                source: SourceFilter::All,
                category: Category::All,
                hide_tools: false,
                members: &[],
                infos: &infos,
                keywords: None,
                sources: &src,
                flags,
                excluded: &[],
                hidden_shaders: &[],
            };
            let r = filter_materials(&l, &index, &p);
            (0..r.len()).filter_map(|i| r.get(&l, i)).map(|e| e.name.clone()).collect::<Vec<_>>()
        };
        assert_eq!(names("is:translucent", None), ["brick/brickwall001a", "wood/woodfloor005a"]);
        assert_eq!(names("is:translucent -is:envmap", None), ["brick/brickwall001a"]);
        let mut bits = MaterialFlagsIndex {
            bits: vec![0; l.len()],
            shaders: vec![u16::MAX; l.len()],
            shader_names: vec!["lightmappedgeneric".to_owned(), "vertexlitgeneric".to_owned()],
        };
        let pos = l.iter().position(|e| e.name == "metal/metalfloor001a").unwrap();
        bits.bits[pos] = FLAGS_KNOWN | MatFlag::SelfIllum.bit();
        assert_eq!(names("is:selfillum", Some(&bits)), ["metal/metalfloor001a"]);
        bits.shaders[pos] = 1;
        let wood = l.iter().position(|e| e.name == "wood/woodfloor005a").unwrap();
        bits.shaders[wood] = 0;
        assert_eq!(names("shader:vertexlit", Some(&bits)), ["metal/metalfloor001a"]);
        assert_eq!(names("shader:lightmapped", Some(&bits)), ["wood/woodfloor005a"]);
        assert!(!names("-shader:vertexlitgeneric", Some(&bits)).contains(&"metal/metalfloor001a".to_owned()));
        assert_eq!(bits.shader_counts()[0], ("lightmappedgeneric".to_owned(), 1));
    }

    #[test]
    fn exclusions_hide_folders_but_not_tools() {
        let ex = normalize_exclusions(["Materials\\Brick/", "tools", " ", "brick"]);
        assert_eq!(ex, ["brick", "tools"]);
        assert!(is_excluded("brick/brickwall001a", &ex));
        assert!(!is_excluded("brickwork/x", &ex));
        assert!(!is_excluded("brick", &ex));
        let l = list();
        let q = Query::parse("");
        let infos = HashMap::new();
        let src = sources();
        let index = SearchIndex::build(&l);
        let p = FilterParams {
            query: &q,
            source: SourceFilter::All,
            category: Category::All,
            hide_tools: false,
            members: &[],
            infos: &infos,
            keywords: None,
            sources: &src,
            flags: None,
            excluded: &ex,
            hidden_shaders: &[],
        };
        let r = filter_materials(&l, &index, &p);
        let shown: Vec<&str> = (0..r.len()).filter_map(|i| r.get(&l, i)).map(|e| e.name.as_str()).collect();
        assert!(!shown.contains(&"brick/brickwall001a"));
        assert!(shown.contains(&"tools/toolsnodraw"), "tool textures stay");
    }

    #[test]
    fn flags_of_parsed_materials() {
        let m = hp_vmt::parse(r#"LightmappedGeneric { "$basetexture" "a/b" "$alphatest" 1 "$selfillum" 1 }"#).unwrap();
        let b = flag_bits_of_material(Some(&m), |_| Some(true));
        assert!(b & MatFlag::Alpha.bit() == 0 && b & MatFlag::SelfIllum.bit() != 0);
        assert!(b & MatFlag::Missing.bit() == 0);
        assert!(flag_bits_of_material(Some(&m), |_| Some(false)) & MatFlag::Alpha.bit() != 0);
        assert!(flag_bits_of_material(Some(&m), |_| None) & MatFlag::Missing.bit() != 0);
        assert!(flag_bits_of_material(None, |_| Some(true)) & MatFlag::Missing.bit() != 0);
        let glass = hp_vmt::parse(r#"LightmappedGeneric { "$basetexture" "a/b" "$translucent" 1 }"#).unwrap();
        assert!(flag_bits_of_material(Some(&glass), |_| Some(true)) & MatFlag::Alpha.bit() != 0);
        assert!(
            flag_bits_of_material(Some(&glass), |_| Some(false)) & MatFlag::Alpha.bit() == 0,
            "no alpha flag: opaque"
        );
    }

    #[test]
    fn used_materials_of_test_map() {
        let state = hp_app::testing::test_state();
        let used = used_materials(&state.doc);
        assert_eq!(used, ["concrete/concretefloor001a", "dev/dev_measuregeneric01b", "tools/toolstrigger"]);
    }

    #[test]
    fn big_list_is_fast() {
        let mut l: Vec<AssetEntry> = (0..50_000).map(|i| e(&format!("folder{}/material_{i:05}", i % 97), 1)).collect();
        l.sort();
        let index = SearchIndex::build(&l);
        let q = Query::parse("folder1 material_4");
        let infos = HashMap::new();
        let src = sources();
        let p = FilterParams {
            query: &q,
            source: SourceFilter::Game,
            category: Category::All,
            hide_tools: true,
            members: &[],
            infos: &infos,
            keywords: None,
            sources: &src,
            flags: None,
            excluded: &[],
            hidden_shaders: &[],
        };
        let start = std::time::Instant::now();
        let r = filter_materials(&l, &index, &p);
        let elapsed = start.elapsed();
        assert!(!r.is_empty());
        assert!(elapsed.as_millis() < 500, "{elapsed:?}");
    }
}
