use ahash::AHashMap;

pub(crate) const MAGIC: &[u8; 8] = b"HPSCNIDX";

pub(crate) const VERSION: u32 = 1;

const HEADER_LEN: usize = 8 + 4 + 8 + 8 + 8;

const MAX_TREE_DEPTH: u32 = 96;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Node {
    pub(crate) name: Box<str>,
    pub(crate) link: bool,
    pub(crate) racy: bool,
    pub(crate) mtime: i64,
    pub(crate) files: Vec<Box<str>>,
    pub(crate) dirs: Vec<Node>,
}

impl Node {
    pub(crate) fn file_count(&self) -> usize {
        self.files.len() + self.dirs.iter().map(Node::file_count).sum::<usize>()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) last_used: u64,
    pub(crate) root: Node,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DecodeError {
    Short,
    BadMagic,
    BadVersion(u32),
    BadFingerprint,
    BadChecksum,
    Corrupt(&'static str),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Short => write!(f, "file too short"),
            DecodeError::BadMagic => write!(f, "not an index file"),
            DecodeError::BadVersion(v) => write!(f, "unsupported version {v}"),
            DecodeError::BadFingerprint => write!(f, "made by different scan rules"),
            DecodeError::BadChecksum => write!(f, "checksum mismatch"),
            DecodeError::Corrupt(what) => write!(f, "corrupt data ({what})"),
        }
    }
}

pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn put_signed(out: &mut Vec<u8>, v: i64) {
    put_varint(out, ((v << 1) ^ (v >> 63)) as u64);
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_varint(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

fn put_node(out: &mut Vec<u8>, node: &Node) {
    put_str(out, &node.name);
    out.push(u8::from(node.link) | (u8::from(node.racy) << 1));
    put_signed(out, node.mtime);
    put_varint(out, node.files.len() as u64);
    for f in &node.files {
        put_str(out, f);
    }
    put_varint(out, node.dirs.len() as u64);
    for d in &node.dirs {
        put_node(out, d);
    }
}

pub(crate) fn encode(entries: &AHashMap<String, Entry>, fingerprint: u64) -> Vec<u8> {
    let mut keys: Vec<&String> = entries.keys().collect();
    keys.sort();
    let mut payload = Vec::with_capacity(1 << 20);
    put_varint(&mut payload, keys.len() as u64);
    for key in keys {
        if let Some(entry) = entries.get(key) {
            put_str(&mut payload, key);
            put_varint(&mut payload, entry.last_used);
            put_node(&mut payload, &entry.root);
        }
    }
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&fingerprint.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    out.extend_from_slice(&fnv1a(&payload).to_le_bytes());
    out.extend_from_slice(&payload);
    out
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8, DecodeError> {
        let b = *self.data.get(self.pos).ok_or(DecodeError::Corrupt("end of data"))?;
        self.pos += 1;
        Ok(b)
    }

    fn varint(&mut self) -> Result<u64, DecodeError> {
        let mut value = 0u64;
        let mut shift = 0u32;
        loop {
            let b = self.byte()?;
            if shift >= 64 {
                return Err(DecodeError::Corrupt("varint"));
            }
            value |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
        }
    }

    fn signed(&mut self) -> Result<i64, DecodeError> {
        let v = self.varint()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }

    fn count(&mut self) -> Result<usize, DecodeError> {
        let n = self.varint()?;
        let remaining = self.data.len() - self.pos;
        if n > remaining as u64 {
            return Err(DecodeError::Corrupt("count"));
        }
        Ok(n as usize)
    }

    fn str(&mut self) -> Result<&'a str, DecodeError> {
        let len = self.varint()?;
        let end = usize::try_from(len)
            .ok()
            .and_then(|l| self.pos.checked_add(l))
            .filter(|e| *e <= self.data.len())
            .ok_or(DecodeError::Corrupt("string length"))?;
        let bytes = self.data.get(self.pos..end).ok_or(DecodeError::Corrupt("string"))?;
        self.pos = end;
        std::str::from_utf8(bytes).map_err(|_| DecodeError::Corrupt("utf-8"))
    }

    fn node(&mut self, depth: u32) -> Result<Node, DecodeError> {
        if depth > MAX_TREE_DEPTH {
            return Err(DecodeError::Corrupt("depth"));
        }
        let name: Box<str> = self.str()?.into();
        let flags = self.byte()?;
        let mtime = self.signed()?;
        let nfiles = self.count()?;
        let mut files = Vec::with_capacity(nfiles);
        for _ in 0..nfiles {
            files.push(Box::from(self.str()?));
        }
        let ndirs = self.count()?;
        let mut dirs = Vec::with_capacity(ndirs);
        for _ in 0..ndirs {
            dirs.push(self.node(depth + 1)?);
        }
        Ok(Node { name, link: flags & 1 != 0, racy: flags & 2 != 0, mtime, files, dirs })
    }
}

pub(crate) fn decode(data: &[u8], fingerprint: u64) -> Result<AHashMap<String, Entry>, DecodeError> {
    if data.len() < HEADER_LEN {
        return Err(DecodeError::Short);
    }
    if data.get(..8) != Some(&MAGIC[..]) {
        return Err(DecodeError::BadMagic);
    }
    let u32_at = |at: usize| data.get(at..at + 4).and_then(|b| b.try_into().ok()).map(u32::from_le_bytes);
    let u64_at = |at: usize| data.get(at..at + 8).and_then(|b| b.try_into().ok()).map(u64::from_le_bytes);
    let version = u32_at(8).ok_or(DecodeError::Short)?;
    if version != VERSION {
        return Err(DecodeError::BadVersion(version));
    }
    if u64_at(12).ok_or(DecodeError::Short)? != fingerprint {
        return Err(DecodeError::BadFingerprint);
    }
    let len = u64_at(20).ok_or(DecodeError::Short)?;
    let sum = u64_at(28).ok_or(DecodeError::Short)?;
    let payload = data.get(HEADER_LEN..).ok_or(DecodeError::Short)?;
    if payload.len() as u64 != len {
        return Err(DecodeError::Corrupt("length"));
    }
    if fnv1a(payload) != sum {
        return Err(DecodeError::BadChecksum);
    }
    let mut r = Reader { data: payload, pos: 0 };
    let n = r.count()?;
    let mut map = AHashMap::with_capacity(n);
    for _ in 0..n {
        let key = r.str()?.to_owned();
        let last_used = r.varint()?;
        let root = r.node(0)?;
        map.insert(key, Entry { last_used, root });
    }
    if r.pos != payload.len() {
        return Err(DecodeError::Corrupt("trailing data"));
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> AHashMap<String, Entry> {
        let leaf = Node {
            name: "sub dir".into(),
            link: true,
            racy: false,
            mtime: 1_700_000_000_123_456_789,
            files: vec!["a.vmt".into(), "b\u{e9}.vtf".into()],
            dirs: Vec::new(),
        };
        let root = Node {
            name: "".into(),
            link: false,
            racy: true,
            mtime: -5,
            files: vec!["root.txt".into()],
            dirs: vec![leaf, Node::default()],
        };
        let mut map = AHashMap::new();
        map.insert("game|c:/x".to_owned(), Entry { last_used: 42, root });
        map.insert("mat|d:/y".to_owned(), Entry { last_used: 7, root: Node::default() });
        map
    }

    #[test]
    fn roundtrip_keeps_everything() {
        let map = sample();
        let bytes = encode(&map, 99);
        assert_eq!(decode(&bytes, 99).unwrap(), map);
    }

    #[test]
    fn rejects_foreign_or_changed_rules() {
        let bytes = encode(&sample(), 99);
        assert_eq!(decode(&bytes, 100), Err(DecodeError::BadFingerprint));
        let mut other = bytes.clone();
        other[8] = 9;
        assert_eq!(decode(&other, 99), Err(DecodeError::BadVersion(9)));
        let mut magic = bytes;
        magic[0] = b'X';
        assert_eq!(decode(&magic, 99), Err(DecodeError::BadMagic));
    }

    #[test]
    fn every_truncation_and_bit_flip_is_an_error_not_a_panic() {
        let bytes = encode(&sample(), 1);
        for cut in 0..bytes.len() {
            assert!(decode(&bytes[..cut], 1).is_err(), "cut {cut}");
        }
        for at in 0..bytes.len() {
            let mut bad = bytes.clone();
            bad[at] ^= 0x55;
            assert!(decode(&bad, 1).is_err(), "flip {at}");
        }
    }

    #[test]
    fn hostile_counts_do_not_allocate_or_loop() {
        let mut payload = Vec::new();
        put_varint(&mut payload, u64::MAX);
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&VERSION.to_le_bytes());
        out.extend_from_slice(&1u64.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        out.extend_from_slice(&fnv1a(&payload).to_le_bytes());
        out.extend_from_slice(&payload);
        assert_eq!(decode(&out, 1), Err(DecodeError::Corrupt("count")));
    }

    #[test]
    fn counts_tree_size() {
        let map = sample();
        let root = &map["game|c:/x"].root;
        assert_eq!(root.file_count(), 3);
    }
}
