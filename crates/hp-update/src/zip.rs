use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use flate2::Crc;
use flate2::read::DeflateDecoder;

use crate::error::UpdateError;

const EOCD_SIG: u32 = 0x0605_4b50;
const CENTRAL_SIG: u32 = 0x0201_4b50;
const LOCAL_SIG: u32 = 0x0403_4b50;
const MAX_DIRECTORY: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub method: u16,
    pub compressed: u64,
    pub size: u64,
    pub crc: u32,
    pub header_offset: u64,
    pub encrypted: bool,
}

impl Entry {
    pub fn file_name(&self) -> &str {
        self.name.rsplit(['/', '\\']).next().unwrap_or(&self.name)
    }

    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/') || self.name.ends_with('\\')
    }
}

pub struct Archive<R: Read + Seek> {
    reader: R,
    entries: Vec<Entry>,
}

fn bad(why: impl Into<String>) -> UpdateError {
    UpdateError::BadZip(why.into())
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

impl<R: Read + Seek> Archive<R> {
    pub fn open(mut reader: R) -> Result<Self, UpdateError> {
        let len = reader.seek(SeekFrom::End(0)).map_err(|e| bad(e.to_string()))?;
        if len < 22 {
            return Err(bad("too small to be a zip file"));
        }
        let tail_len = len.min(22 + 65_535);
        reader.seek(SeekFrom::Start(len - tail_len)).map_err(|e| bad(e.to_string()))?;
        let mut tail = vec![0u8; tail_len as usize];
        reader.read_exact(&mut tail).map_err(|e| bad(e.to_string()))?;
        let at = (0..=tail.len() - 22)
            .rev()
            .find(|&i| u32_at(&tail, i) == Some(EOCD_SIG))
            .ok_or_else(|| bad("no end of central directory"))?;
        let count = u16_at(&tail, at + 10).ok_or_else(|| bad("truncated"))?;
        let dir_size = u64::from(u32_at(&tail, at + 12).ok_or_else(|| bad("truncated"))?);
        let dir_offset = u64::from(u32_at(&tail, at + 16).ok_or_else(|| bad("truncated"))?);
        if count == u16::MAX || dir_size == u64::from(u32::MAX) || dir_offset == u64::from(u32::MAX) {
            return Err(bad("zip64 archives are not supported"));
        }
        if dir_size > MAX_DIRECTORY || dir_offset.checked_add(dir_size).is_none_or(|end| end > len) {
            return Err(bad("the central directory is out of range"));
        }
        reader.seek(SeekFrom::Start(dir_offset)).map_err(|e| bad(e.to_string()))?;
        let mut dir = vec![0u8; dir_size as usize];
        reader.read_exact(&mut dir).map_err(|e| bad(e.to_string()))?;
        let mut entries = Vec::with_capacity(usize::from(count));
        let mut pos = 0usize;
        for _ in 0..count {
            if u32_at(&dir, pos) != Some(CENTRAL_SIG) {
                return Err(bad("bad central directory entry"));
            }
            let flags = u16_at(&dir, pos + 8).ok_or_else(|| bad("truncated"))?;
            let method = u16_at(&dir, pos + 10).ok_or_else(|| bad("truncated"))?;
            let crc = u32_at(&dir, pos + 16).ok_or_else(|| bad("truncated"))?;
            let compressed = u64::from(u32_at(&dir, pos + 20).ok_or_else(|| bad("truncated"))?);
            let size = u64::from(u32_at(&dir, pos + 24).ok_or_else(|| bad("truncated"))?);
            let name_len = usize::from(u16_at(&dir, pos + 28).ok_or_else(|| bad("truncated"))?);
            let extra_len = usize::from(u16_at(&dir, pos + 30).ok_or_else(|| bad("truncated"))?);
            let comment_len = usize::from(u16_at(&dir, pos + 32).ok_or_else(|| bad("truncated"))?);
            let header_offset = u64::from(u32_at(&dir, pos + 42).ok_or_else(|| bad("truncated"))?);
            let name_bytes = dir.get(pos + 46..pos + 46 + name_len).ok_or_else(|| bad("truncated name"))?;
            let name = String::from_utf8_lossy(name_bytes).into_owned();
            entries.push(Entry { name, method, compressed, size, crc, header_offset, encrypted: flags & 1 != 0 });
            pos += 46 + name_len + extra_len + comment_len;
        }
        Ok(Archive { reader, entries })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn find_file(&self, file_name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| !e.is_dir() && e.file_name().eq_ignore_ascii_case(file_name))
    }

    pub fn extract_to(&mut self, entry: &Entry, dest: &Path, max_size: u64) -> Result<u64, UpdateError> {
        if entry.encrypted {
            return Err(bad("encrypted entries are not supported"));
        }
        if entry.size > max_size {
            return Err(bad(format!("{} is too large ({} bytes)", entry.name, entry.size)));
        }
        self.reader.seek(SeekFrom::Start(entry.header_offset)).map_err(|e| bad(e.to_string()))?;
        let mut head = [0u8; 30];
        self.reader.read_exact(&mut head).map_err(|e| bad(e.to_string()))?;
        if u32_at(&head, 0) != Some(LOCAL_SIG) {
            return Err(bad("bad local header"));
        }
        let skip = u64::from(u16_at(&head, 26).unwrap_or(0)) + u64::from(u16_at(&head, 28).unwrap_or(0));
        self.reader.seek(SeekFrom::Current(skip as i64)).map_err(|e| bad(e.to_string()))?;
        let source = (&mut self.reader).take(entry.compressed);
        let result = match entry.method {
            0 => write_checked(source, entry, dest),
            8 => write_checked(DeflateDecoder::new(source), entry, dest),
            other => Err(bad(format!("unsupported compression method {other}"))),
        };
        if result.is_err() {
            let _ = std::fs::remove_file(dest);
        }
        result
    }
}

fn write_checked(mut source: impl Read, entry: &Entry, dest: &Path) -> Result<u64, UpdateError> {
    let mut file = std::fs::File::create(dest)?;
    let mut crc = Crc::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = source.read(&mut buf).map_err(|e| bad(format!("{}: {e}", entry.name)))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > entry.size {
            return Err(bad(format!("{} is larger than announced", entry.name)));
        }
        crc.update(&buf[..n]);
        file.write_all(&buf[..n])?;
    }
    file.flush()?;
    if total != entry.size {
        return Err(bad(format!("{} is truncated ({total} of {} bytes)", entry.name, entry.size)));
    }
    if crc.sum() != entry.crc {
        return Err(bad(format!("{} is corrupt (checksum mismatch)", entry.name)));
    }
    Ok(total)
}

#[cfg(test)]
pub(crate) mod fake {
    use std::io::Write;

    use flate2::Compression;
    use flate2::write::DeflateEncoder;

    pub struct Item<'a> {
        pub name: &'a str,
        pub data: &'a [u8],
        pub deflate: bool,
    }

    pub fn build(items: &[Item<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for item in items {
            let mut crc = flate2::Crc::new();
            crc.update(item.data);
            let (method, body) = if item.deflate {
                let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
                enc.write_all(item.data).unwrap();
                (8u16, enc.finish().unwrap())
            } else {
                (0u16, item.data.to_vec())
            };
            let offset = out.len() as u32;
            out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&method.to_le_bytes());
            out.extend_from_slice(&[0u8; 4]);
            out.extend_from_slice(&crc.sum().to_le_bytes());
            out.extend_from_slice(&(body.len() as u32).to_le_bytes());
            out.extend_from_slice(&(item.data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(item.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(item.name.as_bytes());
            out.extend_from_slice(&body);
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&method.to_le_bytes());
            central.extend_from_slice(&[0u8; 4]);
            central.extend_from_slice(&crc.sum().to_le_bytes());
            central.extend_from_slice(&(body.len() as u32).to_le_bytes());
            central.extend_from_slice(&(item.data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(item.name.len() as u16).to_le_bytes());
            central.extend_from_slice(&[0u8; 12]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(item.name.as_bytes());
        }
        let dir_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&(items.len() as u16).to_le_bytes());
        out.extend_from_slice(&(items.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&dir_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::fake::{Item, build};
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::var_os("HP_UPDATE_TEST_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("hp_update_zip_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn extracts_stored_and_deflated_entries() {
        let big: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        let zip = build(&[
            Item { name: "readme.txt", data: b"hello", deflate: false },
            Item { name: "HammerPro/", data: b"", deflate: false },
            Item { name: "HammerPro/hammerpro.exe", data: &big, deflate: true },
        ]);
        let dir = scratch("ok");
        let mut archive = Archive::open(Cursor::new(zip)).unwrap();
        assert_eq!(archive.entries().len(), 3);
        let exe = archive.find_file("HammerPro.EXE").unwrap().clone();
        assert_eq!(exe.file_name(), "hammerpro.exe");
        assert_eq!(exe.method, 8);
        let out = dir.join("out.exe");
        assert_eq!(archive.extract_to(&exe, &out, 1 << 30).unwrap(), big.len() as u64);
        assert_eq!(std::fs::read(&out).unwrap(), big);
        let txt = archive.find_file("readme.txt").unwrap().clone();
        let out = dir.join("readme.txt");
        archive.extract_to(&txt, &out, 1 << 30).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"hello");
        assert!(archive.find_file("HammerPro").is_none(), "directories are not files");
        assert!(archive.find_file("missing.exe").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detects_corruption_and_limits() {
        let mut zip = build(&[Item { name: "hammerpro.exe", data: b"MZ0123456789", deflate: false }]);
        let dir = scratch("bad");
        let out = dir.join("out.exe");
        {
            let mut archive = Archive::open(Cursor::new(zip.clone())).unwrap();
            let e = archive.find_file("hammerpro.exe").unwrap().clone();
            let err = archive.extract_to(&e, &out, 4).unwrap_err();
            assert!(matches!(err, UpdateError::BadZip(m) if m.contains("too large")));
        }
        zip[46] ^= 0xff;
        let mut archive = Archive::open(Cursor::new(zip)).unwrap();
        let e = archive.find_file("hammerpro.exe").unwrap().clone();
        let err = archive.extract_to(&e, &out, 1 << 20).unwrap_err();
        assert!(matches!(err, UpdateError::BadZip(m) if m.contains("checksum")));
        assert!(!out.exists(), "a corrupt file is not left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_files_that_are_not_zips() {
        assert!(Archive::open(Cursor::new(Vec::new())).is_err());
        assert!(Archive::open(Cursor::new(vec![0u8; 100])).is_err());
        assert!(Archive::open(Cursor::new(b"MZ this is an exe, not a zip file at all".to_vec())).is_err());
        let zip = build(&[Item { name: "a.txt", data: b"x", deflate: false }]);
        assert!(Archive::open(Cursor::new(zip[..zip.len() - 10].to_vec())).is_err());
    }
}
