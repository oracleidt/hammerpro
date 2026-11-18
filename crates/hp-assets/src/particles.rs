use std::collections::HashSet;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use hp_app::state::{ParticleCatalog, ParticleSystemInfo};
use hp_fs::GameFs;

const MANIFEST: &str = "particles/particles_manifest.txt";
const MAX_FILE_BYTES: usize = 64 << 20;
const MAX_FILES: usize = 20_000;
const MAX_SYSTEMS: usize = 200_000;

pub(crate) fn manifest_files(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("");
        let tokens: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
        let [key, value, ..] = tokens.as_slice() else { continue };
        if !key.trim().eq_ignore_ascii_case("file") {
            continue;
        }
        let path = hp_fs::normalize_path(value.trim().trim_start_matches('!')).trim_matches('/').to_owned();
        if path.ends_with(".pcf") && !out.contains(&path) {
            out.push(path);
            if out.len() >= MAX_FILES {
                break;
            }
        }
    }
    out
}

pub(crate) fn build_catalog(fs: &GameFs) -> ParticleCatalog {
    let mut order = fs.read_text(MANIFEST).map(|t| manifest_files(&t)).unwrap_or_default();
    let mut seen: HashSet<String> = order.iter().cloned().collect();
    for e in fs.list("particles", Some("pcf")) {
        if order.len() >= MAX_FILES {
            break;
        }
        if seen.insert(e.name.clone()) {
            order.push(e.name);
        }
    }
    let mut catalog = ParticleCatalog::default();
    let mut names: HashSet<String> = HashSet::new();
    for file in order {
        let Some(source) = fs.which_source(&file) else { continue };
        let parsed = fs
            .read_with(&file, |bytes| if bytes.len() > MAX_FILE_BYTES { None } else { hp_fs::deps::parse_pcf(bytes) });
        let Some(pcf) = parsed.flatten() else { continue };
        for (name, material) in pcf.systems {
            if catalog.systems.len() >= MAX_SYSTEMS {
                break;
            }
            if name.trim().is_empty() || !names.insert(name.to_ascii_lowercase()) {
                continue;
            }
            catalog.systems.push(ParticleSystemInfo { name, file: file.clone(), material, source });
        }
        catalog.files.push(file);
    }
    catalog.systems.sort_by(|a, b| {
        a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase()).then_with(|| a.name.cmp(&b.name))
    });
    catalog
}

#[derive(Default)]
pub(crate) struct ParticleIndex {
    key: Option<(u64, u64)>,
    ready: Option<Arc<ParticleCatalog>>,
    pending: Option<Receiver<ParticleCatalog>>,
}

impl ParticleIndex {
    pub(crate) fn get(&mut self, fs: Option<&GameFs>, generation: u64) -> Option<Arc<ParticleCatalog>> {
        let fs = fs?;
        let key = (generation, fs.generation());
        if self.key != Some(key) {
            self.key = Some(key);
            self.ready = None;
            let (tx, rx) = std::sync::mpsc::channel();
            let fs = fs.clone();
            let spawned = std::thread::Builder::new().name("hp-particle-index".to_owned()).spawn(move || {
                hp_sched::lower_current_thread(hp_sched::Level::BelowNormal);
                let _ = tx.send(build_catalog(&fs));
            });
            match spawned {
                Ok(_) => self.pending = Some(rx),
                Err(e) => {
                    log::warn!("cannot start the particle index: {e}");
                    self.pending = None;
                    self.ready = Some(Arc::new(ParticleCatalog::default()));
                }
            }
        }
        if let Some(rx) = &self.pending {
            match rx.try_recv() {
                Ok(catalog) => {
                    self.ready = Some(Arc::new(catalog));
                    self.pending = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.ready = Some(Arc::new(ParticleCatalog::default()));
                }
            }
        }
        self.ready.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcf(systems: &[(&str, &str)]) -> Vec<u8> {
        const TYPE_STRING: u8 = 5;
        const TYPE_ELEMENT_ARRAY: u8 = 14 + 1;
        let mut out = b"<!-- dmx encoding binary 2 format pcf 1 -->\n\0".to_vec();
        let table = ["DmElement", "particleSystemDefinitions", "DmeParticleSystemDefinition", "material", "radius"];
        out.extend_from_slice(&(table.len() as u16).to_le_bytes());
        for s in table {
            out.extend_from_slice(s.as_bytes());
            out.push(0);
        }
        out.extend_from_slice(&(systems.len() as i32 + 1).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(b"root\0");
        out.extend_from_slice(&[1; 16]);
        for (name, _) in systems {
            out.extend_from_slice(&2u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            out.extend_from_slice(&[2; 16]);
        }
        out.extend_from_slice(&1i32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.push(TYPE_ELEMENT_ARRAY);
        out.extend_from_slice(&(systems.len() as i32).to_le_bytes());
        for i in 0..systems.len() {
            out.extend_from_slice(&(i as i32 + 1).to_le_bytes());
        }
        for (_, material) in systems {
            out.extend_from_slice(&2i32.to_le_bytes());
            out.extend_from_slice(&3u16.to_le_bytes());
            out.push(TYPE_STRING);
            out.extend_from_slice(material.as_bytes());
            out.push(0);
            out.extend_from_slice(&4u16.to_le_bytes());
            out.push(3);
            out.extend_from_slice(&5.0f32.to_le_bytes());
        }
        out
    }

    #[test]
    fn real_gmod_particles() {
        let root = std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod");
        if !root.join("garrysmod").is_dir() {
            return;
        }
        let opts = hp_fs::MountOptions {
            mount_addons: false,
            mount_workshop: false,
            mount_cfg: false,
            ..hp_fs::MountOptions::default()
        };
        let Ok(fs) = GameFs::for_gmod(root, &opts) else { return };
        let t = std::time::Instant::now();
        let c = build_catalog(&fs);
        eprintln!("{} particle systems in {} files, {:?}", c.systems.len(), c.files.len(), t.elapsed());
        assert!(c.systems.len() > 100, "{}", c.systems.len());
        assert!(c.systems.windows(2).all(|w| w[0].name.to_ascii_lowercase() <= w[1].name.to_ascii_lowercase()));
    }

    #[test]
    fn manifest_lines() {
        let text = "particles_manifest\n{\n\t\"file\" \"!particles/error.pcf\"\n\t\"file\"\t\"Particles\\\\Fire_01.pcf\" // comment\n\t\"file\" \"particles/fire_01.pcf\"\n\t\"other\" \"x.pcf\"\n\t\"file\" \"readme.txt\"\n}\n";
        assert_eq!(manifest_files(text), ["particles/error.pcf", "particles/fire_01.pcf"]);
        assert!(manifest_files("").is_empty());
        assert!(manifest_files("\"file").is_empty());
    }

    #[test]
    fn catalog_from_a_game_folder() {
        let dir = std::env::temp_dir().join(format!("hp-assets-particles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("particles").join("sub"));
        let write = |rel: &str, bytes: &[u8]| {
            let _ = std::fs::write(dir.join(rel), bytes);
        };
        write("particles/particles_manifest.txt", b"particles_manifest { \"file\" \"particles/b.pcf\" }");
        write("particles/a.pcf", &pcf(&[("fire_small", "particle/fire"), ("shared", "a/shared")]));
        write("particles/b.pcf", &pcf(&[("Shared", "b/shared"), ("smoke", "particle/smoke")]));
        write("particles/sub/broken.pcf", b"<!-- dmx encoding binary 2 format pcf 1 -->\n\0garbage");
        let fs = GameFs::empty();
        assert!(fs.add_game_dir(&dir).is_ok());
        let c = build_catalog(&fs);
        let names: Vec<&str> = c.systems.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["fire_small", "Shared", "smoke"]);
        let shared = &c.systems[1];
        assert_eq!(shared.file, "particles/b.pcf");
        assert_eq!(c.files[0], "particles/b.pcf");
        assert!(!c.files.contains(&"particles/sub/broken.pcf".to_owned()), "unreadable files are skipped");
        let mut index = ParticleIndex::default();
        let mut got = None;
        for _ in 0..200 {
            got = index.get(Some(&fs), 1);
            if got.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(got.map(|c| c.systems.len()), Some(3));
        assert!(index.get(None, 1).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
