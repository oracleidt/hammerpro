use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use ahash::AHashMap;
use hp_vmt::Material;
use parking_lot::RwLock;

use crate::fs::GameFs;

pub(crate) const MAX_CACHED_MATERIALS: usize = 4096;

struct Cached {
    material: Option<Arc<Material>>,
    used: AtomicU64,
}

#[derive(Default)]
struct CacheState {
    generation: u64,
    map: AHashMap<Box<str>, Cached>,
}

#[derive(Default)]
pub(crate) struct MaterialCache {
    state: RwLock<CacheState>,
    clock: AtomicU64,
}

impl MaterialCache {
    fn tick(&self) -> u64 {
        self.clock.fetch_add(1, Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.state.read().map.len()
    }
}

fn evict_oldest(map: &mut AHashMap<Box<str>, Cached>, keep: usize) {
    if map.len() <= keep {
        return;
    }
    let mut ages: Vec<u64> = map.values().map(|c| c.used.load(Ordering::Relaxed)).collect();
    let cut = map.len() - keep;
    ages.select_nth_unstable(cut - 1);
    let limit = ages[cut - 1];
    let mut removed = 0;
    map.retain(|_, c| {
        if removed < cut && c.used.load(Ordering::Relaxed) <= limit {
            removed += 1;
            false
        } else {
            true
        }
    });
}

impl GameFs {
    pub fn load_material(&self, name: &str) -> Option<Material> {
        self.material(name).map(|m| (*m).clone())
    }

    pub fn material(&self, name: &str) -> Option<Arc<Material>> {
        let key = hp_vmt::normalize_material_name(name);
        if key.is_empty() {
            return None;
        }
        let generation = self.generation();
        let cache = &self.inner.materials;
        {
            let state = cache.state.read();
            if state.generation == generation
                && let Some(hit) = state.map.get(key.as_str())
            {
                hit.used.store(cache.tick(), Ordering::Relaxed);
                return hit.material.clone();
            }
        }
        let loaded = self.load_material_uncached(&key).map(Arc::new);
        let mut state = cache.state.write();
        if state.generation < generation {
            state.map.clear();
            state.generation = generation;
        }
        if state.generation == generation {
            if state.map.len() >= MAX_CACHED_MATERIALS {
                evict_oldest(&mut state.map, MAX_CACHED_MATERIALS * 3 / 4);
            }
            let used = AtomicU64::new(cache.tick());
            state.map.insert(key.into_boxed_str(), Cached { material: loaded.clone(), used });
        }
        loaded
    }

    pub fn material_source(&self, name: &str) -> Option<u32> {
        let key = hp_vmt::normalize_material_name(name);
        if key.is_empty() {
            return None;
        }
        self.which_source(&format!("materials/{key}.vmt"))
    }

    fn load_material_uncached(&self, key: &str) -> Option<Material> {
        let parsed = self.read_with(&format!("materials/{key}.vmt"), hp_vmt::parse_bytes)?;
        let material = match parsed {
            Ok(m) => m,
            Err(e) => {
                log::debug!("cannot parse material {key}: {e}");
                return None;
            }
        };
        if !material.is_patch() {
            return Some(material);
        }
        match hp_vmt::resolve_patch(&material, &|path| self.read_text(path)) {
            Ok(resolved) => Some(resolved),
            Err(e) => {
                log::debug!("cannot resolve patch material {key}: {e}");
                Some(material)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::GameFs;
    use crate::test_util::TempDir;

    #[test]
    fn loads_resolves_patches_and_caches() {
        let tmp = TempDir::new("material");
        tmp.write(
            "g/materials/base/floor.vmt",
            b"\"LightmappedGeneric\" { \"$basetexture\" \"base/floor\" \"$surfaceprop\" \"concrete\" }",
        );
        tmp.write(
            "g/materials/maps/x/floor.vmt",
            b"patch { include \"materials/base/floor.vmt\" insert { \"$envmap\" \"env_cubemap\" } }",
        );
        tmp.write("g/materials/broken/patch.vmt", b"patch { include \"materials/missing.vmt\" }");
        let fs = GameFs::empty();
        fs.add_game_dir(&tmp.path().join("g")).unwrap();

        let m = fs.load_material("Materials\\Base\\Floor.vmt").unwrap();
        assert_eq!(m.base_texture().as_deref(), Some("base/floor"));
        let p = fs.material("maps/x/floor").unwrap();
        assert_eq!(p.shader, "LightmappedGeneric");
        assert_eq!(p.get("$envmap"), Some("env_cubemap"));
        assert_eq!(p.base_texture().as_deref(), Some("base/floor"));
        assert!(std::sync::Arc::ptr_eq(&p, &fs.material("maps/x/floor").unwrap()));
        assert!(fs.material("broken/patch").unwrap().is_patch());
        assert!(fs.material("missing").is_none());
        assert!(fs.material("").is_none());

        tmp.write("g/materials/missing.vmt", b"\"UnlitGeneric\" { }");
        fs.refresh();
        assert!(fs.material("missing").is_some());
        assert!(!fs.material("broken/patch").unwrap().is_patch());
    }

    #[test]
    fn the_cache_stays_bounded_and_keeps_recent_materials() {
        let tmp = TempDir::new("material-bound");
        let n = super::MAX_CACHED_MATERIALS + 10;
        for i in 0..n {
            tmp.write(&format!("g/materials/b/{i}.vmt"), format!("UnlitGeneric {{ $basetexture b/{i} }}").as_bytes());
        }
        let fs = GameFs::empty();
        fs.add_game_dir(&tmp.path().join("g")).unwrap();
        let first = fs.material("b/0").unwrap();
        for i in 1..n {
            assert!(fs.material(&format!("b/{i}")).is_some());
            if i % 64 == 0 {
                assert!(std::sync::Arc::ptr_eq(&first, &fs.material("b/0").unwrap()), "used often: kept");
            }
        }
        assert!(fs.inner.materials.len() <= super::MAX_CACHED_MATERIALS);
        assert!(std::sync::Arc::ptr_eq(&first, &fs.material("b/0").unwrap()));
        assert!(fs.material("b/1").is_some(), "an evicted material loads again");
    }

    #[test]
    fn concurrent_lookups() {
        use rayon::prelude::*;
        let tmp = TempDir::new("material-par");
        for i in 0..50 {
            tmp.write(
                &format!("g/materials/m/{i}.vmt"),
                format!("\"UnlitGeneric\" {{ \"$basetexture\" \"m/{i}\" }}").as_bytes(),
            );
        }
        let fs = GameFs::empty();
        fs.add_game_dir(&tmp.path().join("g")).unwrap();
        let found = (0..2000).into_par_iter().filter(|i| fs.material(&format!("m/{}", i % 60)).is_some()).count();
        assert_eq!(found, 33 * 50 + 20);
    }
}
