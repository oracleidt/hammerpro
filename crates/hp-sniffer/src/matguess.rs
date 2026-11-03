use std::collections::HashMap;

use hp_fs::GameFs;

use crate::memmap::MemoryWorld;
use crate::procmem::MemSource;
use crate::unprotect::is_garbage_name;

pub const MATERIAL_STRIDE: u64 = 0xe0;
pub const MAX_SAME_REFLECTIVITY: usize = 4;
pub const REFLECTIVITY_AT: u64 = 0xa8;

pub fn collect_signatures<S: MemSource + ?Sized>(src: &S, world: &MemoryWorld) -> Vec<(String, [f32; 3])> {
    let Some(render) = &world.render else { return Vec::new() };
    let names = &world.cm.cm.surface_names;
    if render.texinfo_ranks.len() != render.texinfo.len() {
        return Vec::new();
    }
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (t, rank) in render.texinfo.iter().zip(&render.texinfo_ranks) {
        if *rank < 0 || !seen.insert(t.material) {
            continue;
        }
        let Some(name) = names.get(*rank as usize) else { continue };
        if !is_garbage_name(name) {
            continue;
        }
        let head = src.read_vec(t.material, 16);
        if head.len() != 16 {
            continue;
        }
        let real = u64::from_le_bytes(head[8..16].try_into().expect("8 bytes"));
        let raw = src.read_vec(real + REFLECTIVITY_AT, 12);
        if raw.len() != 12 {
            continue;
        }
        let v = [
            f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
            f32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]),
            f32::from_le_bytes([raw[8], raw[9], raw[10], raw[11]]),
        ];
        if v.iter().all(|c| c.is_finite() && (0.0..=64.0).contains(c)) {
            out.push((name.to_ascii_lowercase(), v));
        }
    }
    out
}

fn key(v: [f32; 3]) -> [u32; 3] {
    [v[0].to_bits(), v[1].to_bits(), v[2].to_bits()]
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GuessReport {
    pub indexed_textures: usize,
    pub exact: usize,
    pub ambiguous: usize,
    pub missing: usize,
}

fn load_cache(path: &std::path::Path) -> HashMap<String, [u32; 3]> {
    let mut out = HashMap::new();
    let Ok(text) = std::fs::read_to_string(path) else { return out };
    for line in text.lines() {
        let mut it = line.split('|');
        let (Some(n), Some(a), Some(b), Some(c)) = (it.next(), it.next(), it.next(), it.next()) else { continue };
        if let (Ok(a), Ok(b), Ok(c)) = (a.parse::<u32>(), b.parse::<u32>(), c.parse::<u32>()) {
            out.insert(n.to_owned(), [a, b, c]);
        }
    }
    out
}

fn save_cache(path: &std::path::Path, map: &HashMap<String, [u32; 3]>) {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    let mut text = String::with_capacity(map.len() * 48);
    for k in keys {
        let v = map[k];
        text.push_str(&format!("{k}|{}|{}|{}", v[0], v[1], v[2]));
        text.push(char::from(10u8));
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, text);
}

pub fn resolve(
    fs: &GameFs,
    sigs: &[(String, [f32; 3])],
    cache: Option<&std::path::Path>,
) -> (HashMap<String, String>, GuessReport) {
    let mut report = GuessReport::default();
    let mut known: HashMap<String, [u32; 3]> = cache.map(load_cache).unwrap_or_default();
    let before = known.len();
    let mut materials: std::collections::HashSet<String> = std::collections::HashSet::new();
    for e in fs.list("materials", Some("vmt")) {
        materials.insert(strip(&e.name, ".vmt"));
    }
    let mut textures: Vec<String> = Vec::new();
    for e in fs.list("materials", Some("vtf")) {
        textures.push(strip(&e.name, ".vtf"));
    }
    textures.sort();
    textures.dedup();
    report.indexed_textures = textures.len();
    for tex in &textures {
        if known.contains_key(tex) {
            continue;
        }
        let v = fs.texture_info(tex).map_or([u32::MAX; 3], |i| key(i.reflectivity));
        known.insert(tex.clone(), v);
    }
    if let Some(path) = cache
        && known.len() != before
    {
        save_cache(path, &known);
    }
    let mut index: HashMap<[u32; 3], Vec<String>> = HashMap::new();
    for t in &textures {
        if let Some(k) = known.get(t)
            && *k != [u32::MAX; 3]
        {
            index.entry(*k).or_default().push(t.clone());
        }
    }
    let mut out = HashMap::new();
    for (name, refl) in sigs {
        let Some(list) = index.get(&key(*refl)) else {
            report.missing += 1;
            continue;
        };
        if list.len() > MAX_SAME_REFLECTIVITY {
            report.missing += 1;
            continue;
        }
        let mut named: Vec<&String> = list.iter().filter(|t| materials.contains(*t)).collect();
        named.sort();
        let pick = match named.as_slice() {
            [one] => Some((*one).clone()),
            [] => None,
            many => {
                report.ambiguous += 1;
                Some(many[0].clone())
            }
        };
        match pick {
            Some(p) => {
                report.exact += 1;
                out.insert(name.clone(), p);
            }
            None => report.missing += 1,
        }
    }
    (out, report)
}

fn strip(name: &str, ext: &str) -> String {
    let n = name.replace('\\', "/").to_ascii_lowercase();
    let n = n.strip_prefix("materials/").unwrap_or(&n);
    n.strip_suffix(ext).unwrap_or(n).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_normalized() {
        assert_eq!(strip(&format!("materials{0}Concrete{0}Floor01.vtf", char::from(92u8)), ".vtf"), "concrete/floor01");
        assert_eq!(strip("materials/a/b.vmt", ".vmt"), "a/b");
        assert_eq!(strip("a/b", ".vmt"), "a/b");
    }

    #[test]
    fn the_texture_cache_round_trips() {
        let dir = std::env::temp_dir().join(format!("hp_matguess_{}", std::process::id()));
        let path = dir.join("cache.txt");
        let mut m: HashMap<String, [u32; 3]> = HashMap::new();
        m.insert("a/b".to_owned(), [1, 2, 3]);
        m.insert("c".to_owned(), [u32::MAX; 3]);
        save_cache(&path, &m);
        assert_eq!(load_cache(&path), m);
        assert!(load_cache(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reflectivity_keys_compare_bit_for_bit() {
        assert_eq!(key([0.5, 0.25, 1.0]), key([0.5, 0.25, 1.0]));
        assert_ne!(key([0.5, 0.25, 1.0]), key([0.5, 0.25, 1.0000001]));
    }
}
