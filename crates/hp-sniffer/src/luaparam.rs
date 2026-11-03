use std::collections::{HashMap, HashSet};

use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

const ADDRESS_MASK: u64 = (1 << 47) - 1;
const TAG_SHIFT: u32 = 47;
const TAG_STRING: u64 = 0x1fffb;
const TAG_TABLE: u64 = 0x1fff4;
const TAG_NIL: u64 = 0x1ffff;
const TAG_FALSE: u64 = 0x1fffe;
const TAG_TRUE: u64 = 0x1fffd;
const FIRST_TAG: u64 = 0x1fff0;
const STRING_HEADER: usize = 24;
const STRING_TYPE_AT: usize = 9;
const STRING_TYPE: u8 = 4;
const STRING_LENGTH_AT: usize = 16;
const TABLE_ARRAY_AT: usize = 16;
const TABLE_NODE_AT: usize = 40;
const TABLE_ARRAY_SIZE_AT: usize = 48;
const TABLE_HASH_MASK_AT: usize = 52;
const TABLE_SIZE: usize = 56;
const NODE_SIZE: usize = 24;
const MAX_ENTRIES: usize = 256;
const MAX_STRING: usize = 512;
const MAX_DEPTH: usize = 3;
const PREFIX: usize = 6;
const VALUES_KEY: &str = "Values";

fn u64_at(d: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn tag_of(v: u64) -> u64 {
    v >> TAG_SHIFT
}

fn address_of(v: u64) -> u64 {
    v & ADDRESS_MASK
}

fn string_value(address: u64) -> u64 {
    (TAG_STRING << TAG_SHIFT) | address
}

pub fn read_string<S: MemSource + ?Sized>(src: &S, value: u64) -> Option<String> {
    if tag_of(value) != TAG_STRING {
        return None;
    }
    let at = address_of(value);
    let head = src.read_vec(at, STRING_HEADER);
    if head.len() != STRING_HEADER || head[STRING_TYPE_AT] != STRING_TYPE {
        return None;
    }
    let len = usize::try_from(u32_at(&head, STRING_LENGTH_AT)).ok().filter(|l| *l <= MAX_STRING)?;
    let raw = src.read_vec(at + STRING_HEADER as u64, len);
    (raw.len() == len).then(|| String::from_utf8_lossy(&raw).into_owned())
}

pub fn number_text(v: f64) -> String {
    let single = v as f32;
    if f64::from(single) == v || (f64::from(single) - v).abs() < 1.0e-6 * v.abs().max(1.0) {
        if single == single.trunc() && single.abs() < 1.0e9 {
            format!("{}", single as i64)
        } else {
            format!("{single}")
        }
    } else if v == v.trunc() && v.abs() < 1.0e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn value_text<S: MemSource + ?Sized>(src: &S, value: u64) -> Option<String> {
    match tag_of(value) {
        TAG_STRING => read_string(src, value),
        TAG_TRUE => Some("1".to_owned()),
        TAG_FALSE => Some("0".to_owned()),
        t if t < FIRST_TAG => Some(number_text(f64::from_bits(value))),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum LuaItem {
    Pair(String, String),
    Block(String, Vec<LuaItem>),
}

type TableParts = (Vec<u8>, usize, Option<(u64, u32)>);

fn nodes_of<S: MemSource + ?Sized>(src: &S, table: u64) -> Option<TableParts> {
    let head = src.read_vec(table, TABLE_SIZE);
    if head.len() != TABLE_SIZE {
        return None;
    }
    let node = u64_at(&head, TABLE_NODE_AT);
    let hash = usize::try_from(u32_at(&head, TABLE_HASH_MASK_AT)).ok()?.checked_add(1)?;
    let array_size = u32_at(&head, TABLE_ARRAY_SIZE_AT);
    if hash > MAX_ENTRIES || array_size as usize > MAX_ENTRIES {
        return None;
    }
    let raw = if node == 0 { Vec::new() } else { src.read_vec(node, hash * NODE_SIZE) };
    if raw.len() != hash * NODE_SIZE && node != 0 {
        return None;
    }
    let array = (array_size > 0).then(|| (u64_at(&head, TABLE_ARRAY_AT), array_size));
    Some((raw, hash, array))
}

pub fn decode_table<S: MemSource + ?Sized>(src: &S, table: u64, depth: usize) -> Option<Vec<LuaItem>> {
    let (raw, hash, array) = nodes_of(src, table)?;
    let mut out = Vec::new();
    if let Some((at, size)) = array {
        let elements = src.read_vec(at, size as usize * 8);
        for (i, e) in elements.chunks_exact(8).enumerate().skip(1) {
            if let Some(v) = value_text(src, u64_at(e, 0)) {
                out.push(LuaItem::Pair(i.to_string(), v));
            }
        }
    }
    for i in 0..hash {
        if raw.is_empty() {
            break;
        }
        let (value, key) = (u64_at(&raw, i * NODE_SIZE), u64_at(&raw, i * NODE_SIZE + 8));
        if tag_of(key) == TAG_NIL || tag_of(value) == TAG_NIL {
            continue;
        }
        let Some(k) = value_text(src, key) else { continue };
        if tag_of(value) == TAG_TABLE {
            if depth < MAX_DEPTH
                && let Some(items) = decode_table(src, address_of(value), depth + 1)
            {
                out.push(LuaItem::Block(k, items));
            }
            continue;
        }
        if let Some(v) = value_text(src, value) {
            out.push(LuaItem::Pair(k, v));
        }
    }
    out.sort_by(|a, b| item_key(a).cmp(item_key(b)));
    Some(out)
}

fn item_key(i: &LuaItem) -> &str {
    match i {
        LuaItem::Pair(k, _) | LuaItem::Block(k, _) => k,
    }
}

pub fn find_strings<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions, texts: &[String]) -> HashMap<String, u64> {
    let mut prefixes: HashSet<Vec<u8>> = HashSet::new();
    for t in texts {
        prefixes.insert(t.as_bytes()[..t.len().min(PREFIX)].to_vec());
    }
    let finders: Vec<memchr::memmem::Finder<'static>> =
        prefixes.iter().map(|p| memchr::memmem::Finder::new(p).into_owned()).collect();
    let wanted: HashSet<&str> = texts.iter().map(String::as_str).collect();
    let scan = ScanOptions { overlap: MAX_STRING + STRING_HEADER, ..*opts };
    let found: Vec<Vec<(String, u64)>> = par_chunks(src, &scan, |base, data, own| {
        let mut out = Vec::new();
        for f in &finders {
            for j in f.find_iter(data) {
                if j >= own {
                    break;
                }
                if j < STRING_HEADER || data[j - STRING_HEADER + STRING_TYPE_AT] != STRING_TYPE {
                    continue;
                }
                let len = u32_at(data, j - STRING_HEADER + STRING_LENGTH_AT) as usize;
                let Some(chars) = data.get(j..j + len) else { continue };
                if let Ok(text) = std::str::from_utf8(chars)
                    && wanted.contains(text)
                {
                    out.push((text.to_owned(), base + (j - STRING_HEADER) as u64));
                }
            }
        }
        out
    });
    found.into_iter().flatten().collect()
}

pub fn find_references<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    strings: &HashMap<String, u64>,
) -> HashMap<String, Vec<u64>> {
    let by_value: HashMap<u64, &String> = strings.iter().map(|(t, a)| (string_value(*a), t)).collect();
    let scan = ScanOptions { overlap: 8, writable_only: true, ..*opts };
    let hits: Vec<Vec<(String, u64)>> = par_chunks(src, &scan, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = (8 - (base % 8) as usize) % 8;
        while j + 16 <= data.len() && j < limit {
            let v = u64_at(data, j);
            if tag_of(v) == TAG_STRING
                && let Some(t) = by_value.get(&v)
                && j >= 8
            {
                out.push(((*t).clone(), u64_at(data, j - 8)));
            }
            j += 8;
        }
        out
    });
    let mut out: HashMap<String, Vec<u64>> = HashMap::new();
    for (t, value) in hits.into_iter().flatten() {
        if tag_of(value) == TAG_TABLE {
            out.entry(t).or_default().push(address_of(value));
        }
    }
    out
}

pub fn values_of<S: MemSource + ?Sized>(src: &S, table: u64) -> Option<Vec<LuaItem>> {
    let (raw, hash, _) = nodes_of(src, table)?;
    for i in 0..hash {
        if raw.is_empty() {
            break;
        }
        let (value, key) = (u64_at(&raw, i * NODE_SIZE), u64_at(&raw, i * NODE_SIZE + 8));
        if read_string(src, key).as_deref() == Some(VALUES_KEY) && tag_of(value) == TAG_TABLE {
            return decode_table(src, address_of(value), 0);
        }
    }
    None
}

pub fn proxy_values<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    keys: &[String],
) -> HashMap<String, Vec<LuaItem>> {
    let mut out = HashMap::new();
    if keys.is_empty() {
        return out;
    }
    let mut unique: Vec<String> = keys.to_vec();
    unique.sort();
    unique.dedup();
    let strings = find_strings(src, opts, &unique);
    let refs = find_references(src, opts, &strings);
    for (key, tables) in refs {
        for t in tables {
            if let Some(items) = values_of(src, t) {
                out.insert(key.clone(), items);
                break;
            }
        }
    }
    out
}

pub fn pairs_text(items: &[LuaItem]) -> Vec<(String, String)> {
    items
        .iter()
        .filter_map(|i| match i {
            LuaItem::Pair(k, v) => Some((k.clone(), v.clone())),
            LuaItem::Block(..) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    fn gc_string(m: &mut VecMem, at: u64, text: &str) {
        let mut d = vec![0u8; STRING_HEADER];
        d[8] = 1;
        d[STRING_TYPE_AT] = STRING_TYPE;
        d[STRING_LENGTH_AT..STRING_LENGTH_AT + 4].copy_from_slice(&(text.len() as u32).to_le_bytes());
        d.extend_from_slice(text.as_bytes());
        d.push(0);
        m.add(at, d);
    }

    fn s(at: u64) -> u64 {
        string_value(at)
    }

    fn t(at: u64) -> u64 {
        (TAG_TABLE << TAG_SHIFT) | at
    }

    fn table(m: &mut VecMem, at: u64, node: u64, mask: u32) {
        let mut d = vec![0u8; TABLE_SIZE];
        d[TABLE_NODE_AT..TABLE_NODE_AT + 8].copy_from_slice(&node.to_le_bytes());
        d[TABLE_HASH_MASK_AT..TABLE_HASH_MASK_AT + 4].copy_from_slice(&mask.to_le_bytes());
        m.add(at, d);
    }

    fn nodes(m: &mut VecMem, at: u64, entries: &[(u64, u64)]) {
        let mut d = Vec::new();
        for (value, key) in entries {
            d.extend_from_slice(&value.to_le_bytes());
            d.extend_from_slice(&key.to_le_bytes());
            d.extend_from_slice(&0u64.to_le_bytes());
        }
        m.add(at, d);
    }

    const EMPTY: u64 = u64::MAX;

    fn world() -> VecMem {
        let mut m = VecMem::default();
        gc_string(&mut m, 0x1000_0000, "Values");
        gc_string(&mut m, 0x1000_1000, "resultvar");
        gc_string(&mut m, 0x1000_2000, "$color2");
        gc_string(&mut m, 0x1000_3000, "default");
        gc_string(&mut m, 0x1000_4000, "PlayerColormodels/x/y");
        gc_string(&mut m, 0x1000_5000, "name");
        gc_string(&mut m, 0x1000_6000, "PlayerColor");
        nodes(
            &mut m,
            0x2000_1000,
            &[
                (s(0x1000_2000), s(0x1000_1000)),
                (EMPTY, EMPTY),
                (f64::from(0.87f32).to_bits(), s(0x1000_3000)),
                (f64::from(0.27f32).to_bits(), f64::from(0.58f32).to_bits()),
            ],
        );
        table(&mut m, 0x2000_0000, 0x2000_1000, 3);
        nodes(&mut m, 0x2000_3000, &[(t(0x2000_0000), s(0x1000_0000)), (s(0x1000_6000), s(0x1000_5000))]);
        table(&mut m, 0x2000_2000, 0x2000_3000, 1);
        let mut registry = Vec::new();
        registry.extend_from_slice(&t(0x2000_2000).to_le_bytes());
        registry.extend_from_slice(&s(0x1000_4000).to_le_bytes());
        registry.extend_from_slice(&0u64.to_le_bytes());
        m.add(0x3000_0000, registry);
        m
    }

    #[test]
    fn a_lua_string_is_read_from_its_header_and_characters() {
        let m = world();
        assert_eq!(read_string(&m, s(0x1000_2000)).as_deref(), Some("$color2"));
        assert!(read_string(&m, t(0x2000_0000)).is_none());
        assert!(read_string(&m, s(0x2000_0000)).is_none());
    }

    #[test]
    fn numbers_come_back_the_way_the_key_values_wrote_them() {
        assert_eq!(number_text(f64::from(0.87f32)), "0.87");
        assert_eq!(number_text(2.0), "2");
        assert_eq!(number_text(-40.0), "-40");
        assert_eq!(number_text(0.125), "0.125");
        assert_eq!(number_text(1.0e10), "10000000000");
    }

    #[test]
    fn the_values_table_of_a_proxy_is_decoded_with_strings_and_numbers() {
        let m = world();
        let items = decode_table(&m, 0x2000_0000, 0).expect("table");
        assert_eq!(
            pairs_text(&items),
            vec![
                ("0.58".to_owned(), "0.27".to_owned()),
                ("default".to_owned(), "0.87".to_owned()),
                ("resultvar".to_owned(), "$color2".to_owned()),
            ]
        );
    }

    #[test]
    fn a_proxy_is_found_by_the_key_the_proxy_registry_uses() {
        let m = world();
        let keys = vec!["PlayerColormodels/x/y".to_owned(), "NothingLikeThis".to_owned()];
        let found = proxy_values(&m, &ScanOptions::default(), &keys);
        assert_eq!(found.len(), 1);
        let items = &found["PlayerColormodels/x/y"];
        assert!(pairs_text(items).contains(&("resultvar".to_owned(), "$color2".to_owned())));
    }

    #[test]
    fn nested_tables_become_blocks_and_booleans_become_numbers() {
        let mut m = world();
        gc_string(&mut m, 0x1000_7000, "inner");
        gc_string(&mut m, 0x1000_8000, "flag");
        nodes(&mut m, 0x2000_5000, &[((TAG_TRUE << TAG_SHIFT) | 1, s(0x1000_8000))]);
        table(&mut m, 0x2000_4000, 0x2000_5000, 0);
        nodes(&mut m, 0x2000_7000, &[(t(0x2000_4000), s(0x1000_7000))]);
        table(&mut m, 0x2000_6000, 0x2000_7000, 0);
        let items = decode_table(&m, 0x2000_6000, 0).expect("table");
        assert_eq!(
            items,
            vec![LuaItem::Block("inner".to_owned(), vec![LuaItem::Pair("flag".to_owned(), "1".to_owned())])]
        );
    }
}
