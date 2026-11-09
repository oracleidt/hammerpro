use std::fs::{File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, ensure};
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use serde::{Deserialize, Serialize};

use crate::document::{MAX_MAP_BYTES, Snapshot};

pub const MAX_SNAPSHOTS: usize = 10;
pub const SNAPSHOT_INTERVAL_SECONDS: u64 = 600;

#[derive(Clone, Debug)]
pub struct SnapshotRow {
    pub filename: String,
    pub sequence: u64,
    pub created: u64,
    pub revision: u64,
    pub session: String,
    pub bytes: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    version: u32,
    session: String,
    revision: u64,
    vmf: String,
}

#[derive(Clone)]
pub struct SnapshotStore {
    root: PathBuf,
}

fn row(path: &Path) -> Option<SnapshotRow> {
    let filename = path.file_name()?.to_str()?;
    let stem = filename.strip_prefix("hp-team-")?.strip_suffix(".vmf.gz")?;
    let mut fields = stem.split('-');
    let sequence = fields.next()?.parse().ok()?;
    let created = fields.next()?.parse().ok()?;
    let revision = fields.next()?.parse().ok()?;
    let session_hex = fields.next()?;
    if session_hex.len() > 96 || session_hex.len() % 2 != 0 {
        return None;
    }
    let session = String::from_utf8(
        session_hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
            .collect::<Option<Vec<_>>>()?,
    )
    .ok()?;
    let nonce = fields.next()?;
    if fields.next().is_some() || nonce.len() != 12 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let metadata = path.symlink_metadata().ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    Some(SnapshotRow { filename: filename.to_owned(), sequence, created, revision, session, bytes: metadata.len() })
}

impl SnapshotStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn list(&self) -> Result<Vec<SnapshotRow>> {
        if !self.root.exists() {
            return Ok(Vec::new());
        }
        let mut rows: Vec<_> = std::fs::read_dir(&self.root)?
            .filter_map(|entry| entry.ok().and_then(|entry| row(&entry.path())))
            .collect();
        rows.sort_by_key(|row| (row.sequence, row.created, row.filename.clone()));
        rows.reverse();
        Ok(rows)
    }

    pub fn save(&self, map: &Snapshot, session: &str, revision: u64) -> Result<SnapshotRow> {
        map.validate()?;
        ensure!(session.len() <= 48, "Invalid snapshot session name");
        std::fs::create_dir_all(&self.root)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.root.join("snapshots.lock"))?;
        lock.lock()?;
        let rows = self.list()?;
        let sequence = rows
            .iter()
            .map(|row| row.sequence)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("Snapshot sequence exhausted"))?;
        let created = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let random = crate::protocol::random_secret()?;
        let nonce = random[..6].iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let session_hex = session.as_bytes().iter().map(|byte| format!("{byte:02x}")).collect::<String>();
        let filename = format!("hp-team-{sequence:020}-{created:020}-{revision:020}-{session_hex}-{nonce}.vmf.gz");
        let final_path = self.root.join(&filename);
        let partial_path = self.root.join(format!("{filename}.partial"));
        let result = (|| -> Result<()> {
            let file = OpenOptions::new().create_new(true).write(true).open(&partial_path)?;
            let mut encoder = GzEncoder::new(BufWriter::new(file), Compression::new(6));
            serde_json::to_writer(
                &mut encoder,
                &Archive { version: 1, session: session.to_owned(), revision, vmf: map.to_vmf()? },
            )?;
            let mut writer = encoder.finish()?;
            writer.flush()?;
            writer.get_ref().sync_all()?;
            ensure!(!final_path.exists(), "Snapshot name collision");
            std::fs::rename(&partial_path, &final_path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&partial_path);
        }
        result?;
        for obsolete in self.list()?.into_iter().skip(MAX_SNAPSHOTS) {
            let path = self.root.join(&obsolete.filename);
            ensure!(path.parent() == Some(self.root.as_path()), "Invalid snapshot path");
            std::fs::remove_file(path)?;
        }
        row(&final_path).ok_or_else(|| anyhow::anyhow!("Snapshot missing after save"))
    }

    pub fn load(&self, filename: &str) -> Result<Snapshot> {
        ensure!(!filename.contains(['/', '\\', ':']) && filename.len() <= 240, "Invalid snapshot filename");
        let path = self.root.join(filename);
        ensure!(row(&path).is_some(), "Not a session snapshot");
        let mut bytes = Vec::new();
        GzDecoder::new(BufReader::new(File::open(&path)?))
            .take((MAX_MAP_BYTES * 2 + 1024 * 1024) as u64)
            .read_to_end(&mut bytes)?;
        let archive: Archive = serde_json::from_slice(&bytes)?;
        ensure!(archive.version == 1, "Unsupported snapshot version");
        Snapshot::from_vmf(&archive.vmf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressed_snapshots_roundtrip_and_rotate_only_owned_files() {
        let root = std::env::temp_dir().join(format!(
            "hp-collab-snapshots-{}-{}",
            std::process::id(),
            crate::protocol::random_secret().unwrap()[0]
        ));
        std::fs::create_dir_all(&root).unwrap();
        let unrelated = root.join("keep.vmf");
        std::fs::write(&unrelated, "user file").unwrap();
        let store = SnapshotStore::new(root.clone());
        let map = Snapshot::from_vmf("world {\"id\" \"1\" \"classname\" \"worldspawn\"}").unwrap();
        let mut first = String::new();
        for revision in 0..11 {
            let saved = store.save(&map, "My session", revision).unwrap();
            if revision == 0 {
                first = saved.filename.clone();
            }
            assert_eq!(store.load(&saved.filename).unwrap(), map);
        }
        let rows = store.list().unwrap();
        assert_eq!(rows.len(), 10);
        assert_eq!(rows[0].revision, 10);
        assert!(!root.join(first).exists());
        assert_eq!(std::fs::read_to_string(&unrelated).unwrap(), "user file");
        assert!(store.load("../keep.vmf").is_err());
        assert!(store.load("keep.vmf").is_err());
        std::fs::write(root.join(&rows[0].filename), b"damaged gzip").unwrap();
        assert!(store.load(&rows[0].filename).is_err());
        for entry in std::fs::read_dir(&root).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        std::fs::remove_dir(root).unwrap();
    }
}
