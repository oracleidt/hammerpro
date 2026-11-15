use std::collections::HashSet;
use std::path::Path;

use hp_bsp::{Bsp, PakOptions};

pub(crate) fn copy_keeping_cubemaps(from: &Path, to: &Path) -> std::io::Result<Option<usize>> {
    let Some(map) = to.file_stem().and_then(|s| s.to_str()).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if !to.is_file() {
        return Ok(None);
    }
    let Some((new, writer, kept)) = carried(from, to, map) else { return Ok(None) };
    let compressed = new.pakfile().is_ok_and(|p| p.any_compressed());
    let opts = if compressed {
        PakOptions { compression: hp_bsp::PakCompression::Lzma, ..PakOptions::default() }
    } else {
        PakOptions::default()
    };
    let Ok(zip) = writer.finish(opts) else { return Ok(None) };
    let mut edit = new.edit();
    if edit.set_pakfile(zip).is_err() {
        return Ok(None);
    }
    edit.save(to).map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(Some(kept))
}

fn carried(from: &Path, to: &Path, map: &str) -> Option<(Bsp, hp_bsp::PakWriter, usize)> {
    let old = Bsp::open(to).ok()?;
    let old_pak = old.pakfile().ok()?;
    if hp_bsp::cubemaps::count_built(&old_pak, map) == 0 {
        return None;
    }
    let new = Bsp::open(from).ok()?;
    let mut writer = new.pakfile().and_then(|p| p.to_writer()).ok()?;
    let mut done = HashSet::new();
    let kept = hp_bsp::cubemaps::carry_over(&old_pak, &new, map, &mut writer, &mut done).ok()?;
    (kept > 0).then_some((new, writer, kept))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hp_bsp::{PakWriter, lump, synthetic_bsp};

    fn bsp_with(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = PakWriter::new();
        for (name, data) in files {
            w.add(name, data.to_vec()).expect("add");
        }
        let samples: Vec<u8> = [0i32, 0, 64, 0].iter().flat_map(|v| v.to_le_bytes()).collect();
        synthetic_bsp(
            20,
            &[
                (lump::ENTITIES, b"{\n\"classname\" \"worldspawn\"\n}\n\0".to_vec()),
                (lump::CUBEMAPS, samples),
                (lump::PAKFILE, w.finish(PakOptions::default()).expect("pak")),
            ],
        )
    }

    const DEFAULT: &str = "materials/maps/gm_t/cubemapdefault.vtf";
    const SAMPLE: &str = "materials/maps/gm_t/c0_0_64.vtf";
    const PLACEHOLDER: &[u8] = b"VTF\0placeholder";

    #[test]
    fn built_cubemaps_survive_the_copy() {
        let dir = crate::test_util::scratch("cubemaps_copy");
        let (work, maps) = (dir.join("work"), dir.join("maps"));
        std::fs::create_dir_all(&work).expect("work");
        std::fs::create_dir_all(&maps).expect("maps");
        let (from, to) = (work.join("gm_t.bsp"), maps.join("gm_t.bsp"));
        let fresh = bsp_with(&[(DEFAULT, PLACEHOLDER), (SAMPLE, PLACEHOLDER), ("materials/new.vmt", b"new")]);
        std::fs::write(&from, &fresh).expect("write");

        assert_eq!(copy_keeping_cubemaps(&from, &to).expect("copy"), None);
        std::fs::write(&to, &fresh).expect("write");
        assert_eq!(copy_keeping_cubemaps(&from, &to).expect("copy"), None);
        std::fs::write(&to, bsp_with(&[(DEFAULT, PLACEHOLDER), (SAMPLE, &[5; 300])])).expect("write");
        assert_eq!(copy_keeping_cubemaps(&from, &to).expect("copy"), Some(1));
        let written = Bsp::read(&to).expect("read");
        let pak = written.pakfile().expect("pak");
        assert_eq!(pak.read_name(SAMPLE).expect("read").expect("cubemap").as_ref(), &[5; 300][..]);
        assert_eq!(pak.read_name("materials/new.vmt").expect("read").expect("new").as_ref(), b"new");
        assert!(!pak.any_compressed(), "stored: the game builds cubemaps into stored pakfiles only");
        assert_eq!(written.cubemaps().expect("samples"), vec![([0, 0, 64], 0)]);
        std::fs::write(&to, b"VBSP broken").expect("write");
        assert_eq!(copy_keeping_cubemaps(&from, &to).expect("copy"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
