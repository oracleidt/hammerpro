use std::borrow::Cow;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub(super) type Hit = (u32, u16, u16);

pub(super) struct Store {
    budget: usize,
    memory: AtomicUsize,
    disk: AtomicU64,
    spill: Mutex<Option<Arc<Spill>>>,
}

impl Store {
    pub fn new(budget: usize) -> Self {
        Self { budget, memory: AtomicUsize::new(0), disk: AtomicU64::new(0), spill: Mutex::new(None) }
    }

    pub fn usage(&self) -> (usize, u64) {
        (self.memory.load(Ordering::Relaxed), self.disk.load(Ordering::Relaxed))
    }

    pub fn keep(&self, hits: Vec<Hit>) -> Result<Hits, String> {
        let bytes = hits.capacity().saturating_mul(size_of::<Hit>());
        if self
            .memory
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(bytes).filter(|next| *next <= self.budget)
            })
            .is_ok()
        {
            return Ok(Hits::Memory(hits));
        }
        let spill = {
            let mut slot = self.spill.lock().map_err(|error| error.to_string())?;
            if slot.is_none() {
                *slot = Some(Arc::new(Spill::new().map_err(|error| error.to_string())?));
            }
            slot.as_ref().cloned().ok_or("the bounce tile file is unavailable")?
        };
        let offset = spill.write(&hits).map_err(|error| error.to_string())?;
        self.disk.fetch_add((hits.len() as u64) * 8, Ordering::Relaxed);
        Ok(Hits::Disk { spill, offset, len: hits.len() })
    }
}

pub(super) struct Spill {
    path: PathBuf,
    file: Mutex<Option<File>>,
}

impl Spill {
    fn new() -> std::io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let mut error = None;
        for _ in 0..16 {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("hammerpro-light-tiles-{}-{id}.tmp", std::process::id()));
            let mut options = OpenOptions::new();
            options.read(true).write(true).create_new(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                const DELETE_ON_CLOSE: u32 = 0x0400_0000;
                const TEMPORARY: u32 = 0x0000_0100;
                options.custom_flags(DELETE_ON_CLOSE | TEMPORARY);
            }
            match options.open(&path) {
                Ok(file) => {
                    #[cfg(unix)]
                    std::fs::remove_file(&path)?;
                    return Ok(Self { path, file: Mutex::new(Some(file)) });
                }
                Err(failure) if failure.kind() == std::io::ErrorKind::AlreadyExists => error = Some(failure),
                Err(failure) => return Err(failure),
            }
        }
        Err(error.unwrap_or_else(|| std::io::Error::other("cannot create the lighting tile file")))
    }

    fn write(&self, hits: &[Hit]) -> std::io::Result<u64> {
        let mut lock = self.file.lock().map_err(|error| std::io::Error::other(error.to_string()))?;
        let file = lock.as_mut().ok_or_else(|| std::io::Error::other("the lighting tile file is closed"))?;
        let offset = file.seek(SeekFrom::End(0))?;
        let mut buffer = Vec::with_capacity(64 << 10);
        for tile in hits.chunks(8192) {
            buffer.clear();
            for &(face, s, t) in tile {
                buffer.extend_from_slice(&face.to_le_bytes());
                buffer.extend_from_slice(&s.to_le_bytes());
                buffer.extend_from_slice(&t.to_le_bytes());
            }
            file.write_all(&buffer)?;
        }
        Ok(offset)
    }

    fn read(&self, offset: u64, count: usize) -> std::io::Result<Vec<Hit>> {
        let mut bytes = vec![0u8; count * 8];
        {
            let mut lock = self.file.lock().map_err(|error| std::io::Error::other(error.to_string()))?;
            let file = lock.as_mut().ok_or_else(|| std::io::Error::other("the lighting tile file is closed"))?;
            file.seek(SeekFrom::Start(offset))?;
            file.read_exact(&mut bytes)?;
        }
        Ok(bytes
            .chunks_exact(8)
            .map(|record| {
                (
                    u32::from_le_bytes(record[..4].try_into().unwrap()),
                    u16::from_le_bytes(record[4..6].try_into().unwrap()),
                    u16::from_le_bytes(record[6..8].try_into().unwrap()),
                )
            })
            .collect())
    }
}

impl Drop for Spill {
    fn drop(&mut self) {
        self.file.get_mut().unwrap_or_else(|error| error.into_inner()).take();
        let _ = std::fs::remove_file(&self.path);
    }
}

pub(super) enum Hits {
    Memory(Vec<Hit>),
    Disk { spill: Arc<Spill>, offset: u64, len: usize },
}

impl Hits {
    pub fn tiles(&self, size: usize) -> Tiles<'_> {
        Tiles { hits: self, size: size.max(1), cursor: 0 }
    }
}

pub(super) struct Tiles<'a> {
    hits: &'a Hits,
    size: usize,
    cursor: usize,
}

impl<'a> Iterator for Tiles<'a> {
    type Item = Result<(usize, Cow<'a, [Hit]>), String>;

    fn next(&mut self) -> Option<Self::Item> {
        let len = match self.hits {
            Hits::Memory(hits) => hits.len(),
            Hits::Disk { len, .. } => *len,
        };
        if self.cursor >= len {
            return None;
        }
        let start = self.cursor;
        let end = start.saturating_add(self.size).min(len);
        self.cursor = end;
        Some(match self.hits {
            Hits::Memory(hits) => Ok((start, Cow::Borrowed(&hits[start..end]))),
            Hits::Disk { spill, offset, .. } => spill
                .read(offset + start as u64 * 8, end - start)
                .map(|hits| (start, Cow::Owned(hits)))
                .map_err(|error| error.to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spilled_tiles_are_lossless_bounded_and_removed() {
        let source: Vec<_> = (0u32..12000).map(|i| (i.wrapping_mul(981723), i as u16, (i * 7) as u16)).collect();
        let memory = Store::new(1 << 20).keep(source.clone()).expect("memory");
        let store = Store::new(0);
        let disk = store.keep(source.clone()).expect("disk");
        let path = match &disk {
            Hits::Disk { spill, .. } => spill.path.clone(),
            _ => panic!("spilled"),
        };
        let flatten = |hits: &Hits| {
            hits.tiles(513)
                .flat_map(|part| {
                    let (_, part) = part.expect("tile");
                    assert!(part.len() <= 513);
                    part.into_owned()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(flatten(&memory), source);
        assert_eq!(flatten(&disk), source);
        assert_eq!(store.usage(), (0, source.len() as u64 * 8));
        drop(store);
        assert_eq!(flatten(&disk), source);
        drop(disk);
        assert!(!path.exists());
    }
}
