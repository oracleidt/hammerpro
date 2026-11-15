use std::collections::{BTreeSet, HashMap, HashSet};

use crate::error::Result;
use crate::pak::{PakArchive, PakCompression, PakOptions, PakWriter};
use crate::{Bsp, lump};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PakCleanReport {
    pub files_before: usize,
    pub removed: Vec<String>,
    pub pak_before: u64,
    pub pak_after: u64,
    pub bsp_before: u64,
    pub bsp_after: u64,
}

impl PakCleanReport {
    pub fn summary(&self) -> String {
        format!(
            "Pakfile cleaned (-cleanpak): {} of {} files removed as unused, pakfile {} KB -> {} KB (LZMA), BSP {} KB -> {} KB",
            self.removed.len(),
            self.files_before,
            self.pak_before.div_ceil(1024),
            self.pak_after.div_ceil(1024),
            self.bsp_before.div_ceil(1024),
            self.bsp_after.div_ceil(1024)
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Material,
    Texture,
    Model,
    ModelPart,
    Sound,
    Particle,
}

struct Candidate {
    key: String,
    kind: Kind,
    name: String,
    group: String,
}

const TEXT_EXTENSIONS: &[&str] =
    &["vmt", "txt", "res", "lua", "pcf", "mdl", "cfg", "vmf", "rad", "json", "vdf", "kv", "scr", "vcd", "ain", "nav"];

fn extension(key: &str) -> &str {
    let file = key.rsplit('/').next().unwrap_or(key);
    file.rsplit_once('.').map_or("", |(_, e)| e)
}

fn strip_suffix<'a>(path: &'a str, suffixes: &[&str]) -> &'a str {
    suffixes.iter().find_map(|x| path.strip_suffix(x)).unwrap_or(path)
}

fn material_stem(path: &str) -> &str {
    strip_suffix(strip_suffix(path, &[".vmt", ".vtf"]), &[".hdr"])
}

fn model_stem(path: &str) -> &str {
    let base = strip_suffix(path, &[".mdl", ".vvd", ".phy", ".ani", ".vtx"]);
    if path.ends_with(".vtx") { strip_suffix(base, &[".dx90", ".dx80", ".sw", ".xbox"]) } else { base }
}

fn candidate(key: &str, map: &str) -> Option<Candidate> {
    let ext = extension(key);
    if let Some(rest) = key.strip_prefix("materials/") {
        if rest.starts_with(&format!("maps/{map}/")) || rest.starts_with("skybox/") || rest.starts_with("maps/") {
            return None;
        }
        let kind = match ext {
            "vmt" => Kind::Material,
            "vtf" => Kind::Texture,
            _ => return None,
        };
        let name = material_stem(rest).to_owned();
        return Some(Candidate { key: key.to_owned(), kind, group: name.clone(), name });
    }
    if let Some(rest) = key.strip_prefix("models/") {
        let stem = model_stem(rest).to_owned();
        let kind = match ext {
            "mdl" => Kind::Model,
            "vvd" | "vtx" | "phy" | "ani" => Kind::ModelPart,
            _ => return None,
        };
        return Some(Candidate { key: key.to_owned(), kind, name: format!("{stem}.mdl"), group: stem });
    }
    if let Some(rest) = key.strip_prefix("sound/") {
        if !matches!(ext, "wav" | "mp3" | "ogg") {
            return None;
        }
        return Some(Candidate {
            key: key.to_owned(),
            kind: Kind::Sound,
            name: rest.to_owned(),
            group: rest.to_owned(),
        });
    }
    if key.starts_with("particles/") && ext == "pcf" {
        return Some(Candidate {
            key: key.to_owned(),
            kind: Kind::Particle,
            name: key.to_owned(),
            group: key.to_owned(),
        });
    }
    None
}

fn path_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b'\\' | b'+' | b'#' | b'$' | b'@' | b'&')
}

#[derive(Default)]
struct References {
    suffixes: BTreeSet<String>,
}

impl References {
    fn add(&mut self, bytes: &[u8]) {
        let mut i = 0;
        while i < bytes.len() {
            if !path_char(bytes[i]) {
                i += 1;
                continue;
            }
            let start = i;
            while i < bytes.len() && path_char(bytes[i]) {
                i += 1;
            }
            if i - start < 2 || i - start > 512 {
                continue;
            }
            let run: String = bytes[start..i]
                .iter()
                .map(|&b| if b == b'\\' { '/' } else { (b as char).to_ascii_lowercase() })
                .collect();
            let mut from = 0;
            loop {
                let part = run[from..].trim_start_matches('/');
                if !part.is_empty() && !self.suffixes.contains(part) {
                    self.suffixes.insert(part.to_owned());
                }
                match run[from..].find('/') {
                    Some(k) => from += k + 1,
                    None => break,
                }
            }
        }
    }

    fn has_prefix(&self, name: &str) -> bool {
        self.suffixes.range(name.to_owned()..).next().is_some_and(|s| s.starts_with(name))
    }
}

fn file_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

fn parent_dir(name: &str) -> &str {
    name.rsplit_once('/').map_or("", |(d, _)| d)
}

fn referenced(c: &Candidate, refs: &References) -> bool {
    if refs.has_prefix(&c.name) {
        return true;
    }
    if c.kind == Kind::Material || c.kind == Kind::Texture {
        let dir = parent_dir(&c.name);
        let file = file_name(&c.name);
        if c.kind == Kind::Material && !dir.is_empty() && refs.has_prefix(dir) && refs.suffixes.contains(file) {
            return true;
        }
    }
    false
}

pub fn unused_entries(bsp: &Bsp, map: &str) -> Result<Vec<String>> {
    let pak = bsp.pakfile()?;
    unused_in(bsp, &pak, &map.to_ascii_lowercase())
}

fn unused_in(bsp: &Bsp, pak: &PakArchive<'_>, map: &str) -> Result<Vec<String>> {
    let mut refs = References::default();
    refs.add(format!("detail/detailsprites overviews/{map}").as_bytes());
    refs.add(bsp.entity_text().unwrap_or_default().as_bytes());
    for name in bsp.texture_names().unwrap_or_default() {
        refs.add(name.as_bytes());
    }
    if let Ok(lumps) = bsp.game_lumps() {
        for gl in &lumps.lumps {
            if let Ok(data) = gl.data() {
                refs.add(&data);
            }
        }
    }
    if let Ok(data) = bsp.lump(lump::OVERLAYS) {
        refs.add(&data);
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    let mut texts: HashMap<String, usize> = HashMap::new();
    for (i, entry) in pak.entries().iter().enumerate() {
        match candidate(&entry.key, map) {
            Some(c) => candidates.push(c),
            None => {
                if TEXT_EXTENSIONS.contains(&extension(&entry.key)) {
                    texts.insert(entry.key.clone(), i);
                }
            }
        }
    }
    for i in texts.values() {
        let entry = &pak.entries()[*i];
        if let Ok(data) = pak.read(entry) {
            refs.add(&data);
        }
    }
    let models: HashSet<String> =
        candidates.iter().filter(|c| c.kind == Kind::Model).map(|c| c.group.clone()).collect();
    let mut kept: HashSet<String> = HashSet::new();
    let mut kept_groups: HashSet<(bool, String)> = HashSet::new();
    loop {
        let mut grew = false;
        for c in &candidates {
            if kept.contains(&c.key) {
                continue;
            }
            let model_group = matches!(c.kind, Kind::Model | Kind::ModelPart);
            let keep = match c.kind {
                Kind::ModelPart if !models.contains(&c.group) => true,
                Kind::ModelPart => kept_groups.contains(&(true, c.group.clone())),
                _ => kept_groups.contains(&(model_group, c.group.clone())) || referenced(c, &refs),
            };
            if !keep {
                continue;
            }
            kept.insert(c.key.clone());
            kept_groups.insert((model_group, c.group.clone()));
            grew = true;
            if matches!(c.kind, Kind::Material | Kind::Model | Kind::Particle)
                && let Some(entry) = pak.find(&c.key)
                && let Ok(data) = pak.read(entry)
            {
                refs.add(&data);
            }
        }
        if !grew {
            break;
        }
    }
    let mut unused: Vec<String> = candidates.into_iter().filter(|c| !kept.contains(&c.key)).map(|c| c.key).collect();
    unused.sort();
    Ok(unused)
}

pub fn clean(data: Vec<u8>, map: &str) -> Result<(Vec<u8>, PakCleanReport)> {
    let bsp_before = data.len() as u64;
    let bsp = Bsp::from_vec(data)?;
    let map = map.to_ascii_lowercase();
    let (zip, mut report) = {
        let pak = bsp.pakfile()?;
        let removed = unused_in(&bsp, &pak, &map)?;
        let gone: HashSet<&str> = removed.iter().map(String::as_str).collect();
        let mut writer = PakWriter::new();
        for entry in pak.entries() {
            if gone.contains(entry.key.as_str()) {
                continue;
            }
            writer.add(&entry.key, pak.read(entry)?.into_owned())?;
        }
        let zip = writer.finish(PakOptions { compression: PakCompression::Lzma, level: 6 })?;
        let report = PakCleanReport {
            files_before: pak.len(),
            removed,
            pak_before: pak.byte_len() as u64,
            pak_after: zip.len() as u64,
            bsp_before,
            bsp_after: 0,
        };
        (zip, report)
    };
    let mut edit = bsp.edit();
    edit.set_pakfile(zip)?;
    let out = edit.to_vec()?;
    report.bsp_after = out.len() as u64;
    Ok((out, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::synthetic_bsp;

    fn texdata(names: &[&str]) -> (Vec<u8>, Vec<u8>) {
        let mut data = Vec::new();
        let mut table = Vec::new();
        for n in names {
            table.extend_from_slice(&(data.len() as u32).to_le_bytes());
            data.extend_from_slice(n.as_bytes());
            data.push(0);
        }
        (data, table)
    }

    #[test]
    fn unused_files_go_and_everything_referenced_stays() {
        let mut w = PakWriter::new();
        let vmt =
            b"\"LightmappedGeneric\"\n{\n\"$basetexture\" \"custom/wall\"\n\"$bumpmap\" \"custom\\wall_normal\"\n}\n";
        let files: &[(&str, &[u8])] = &[
            ("materials/custom/wall.vmt", vmt),
            ("materials/custom/wall.vtf", &[1; 200]),
            ("materials/custom/wall_normal.vtf", &[2; 200]),
            ("materials/custom/unused.vmt", b"\"UnlitGeneric\" { \"$basetexture\" \"custom/unused\" }"),
            ("materials/custom/unused.vtf", &[3; 200]),
            (
                "materials/models/props/crate/crate.vmt",
                b"\"VertexLitGeneric\" { \"$basetexture\" \"models/props/crate/crate_d\" }",
            ),
            ("materials/models/props/crate/crate_d.vtf", &[4; 200]),
            ("materials/maps/gm_t/c0_0_64.vtf", &[5; 200]),
            ("materials/skybox/sky_dayrt.vtf", &[6; 200]),
            ("models/props/crate.mdl", b"IDST\0crate\0models\\props\\crate\\\0"),
            ("models/props/crate.vvd", &[7; 200]),
            ("models/props/crate.dx90.vtx", &[8; 200]),
            ("models/props/old.mdl", b"IDST\0old\0"),
            ("models/props/old.vvd", &[9; 200]),
            ("sound/ambient/hum.wav", &[10; 200]),
            ("sound/ambient/never.wav", &[11; 200]),
            ("scripts/custom_sounds.txt", b"\"x\" { \"wave\" \")ambient/hum.wav\" }"),
            ("particles/fire.pcf", b"<!-- dmx -->effects/flame"),
            ("particles/unused.pcf", &[12; 200]),
            ("particles/gm_t_manifest.txt", b"particles_manifest { \"file\" \"!particles/fire.pcf\" }"),
            ("materials/effects/flame.vmt", b"\"Sprite\" { \"$basetexture\" \"effects/flame\" }"),
            ("materials/effects/flame.vtf", &[13; 200]),
            ("lua/autorun/x.lua", b"print(1)"),
        ];
        for (n, d) in files {
            w.add(n, d.to_vec()).unwrap();
        }
        let zip = w.finish(PakOptions::default()).unwrap();
        let ents = b"{\n\"classname\" \"worldspawn\"\n}\n{\n\"classname\" \"prop_physics\"\n\"model\" \"models/props/crate.mdl\"\n}\n\0";
        let (sd, st) = texdata(&["CUSTOM/WALL", "maps/gm_t/custom/metal_0_0_64"]);
        let data = synthetic_bsp(
            20,
            &[
                (lump::ENTITIES, ents.to_vec()),
                (lump::TEXDATA_STRING_DATA, sd),
                (lump::TEXDATA_STRING_TABLE, st),
                (lump::PAKFILE, zip),
            ],
        );
        let bsp = Bsp::from_vec(data.clone()).unwrap();
        let unused = unused_entries(&bsp, "gm_t").unwrap();
        assert_eq!(
            unused,
            [
                "materials/custom/unused.vmt",
                "materials/custom/unused.vtf",
                "models/props/old.mdl",
                "models/props/old.vvd",
                "particles/unused.pcf",
                "sound/ambient/never.wav",
            ]
        );
        let (out, report) = clean(data, "gm_t").unwrap();
        assert_eq!(report.removed.len(), 6);
        assert!(report.pak_after < report.pak_before, "{report:?}");
        let cleaned = Bsp::from_vec(out).unwrap();
        let pak = cleaned.pakfile().unwrap();
        assert_eq!(pak.len(), files.len() - 6);
        assert!(pak.any_compressed());
        assert_eq!(pak.read_name("materials/custom/wall.vmt").unwrap().as_deref(), Some(&vmt[..]));
        assert!(!pak.find("sound/ambient/hum.wav").unwrap().is_compressed());
        assert_eq!(cleaned.entity_text().unwrap(), String::from_utf8_lossy(&ents[..ents.len() - 1]));
    }
}
