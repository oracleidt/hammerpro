use std::collections::HashMap;

use glam::Vec3;

use crate::lights::light_for_string;

pub const MAX_RAD_BYTES: usize = 4 << 20;
const MAX_ENTRIES: usize = 65_536;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TexLights {
    map: HashMap<String, Vec3>,
    hdr: HashMap<String, Vec3>,
}

pub fn normalize_material(name: &str) -> String {
    let mut s = name.trim().replace('\\', "/").to_ascii_lowercase();
    if let Some(rest) = s.strip_prefix("materials/") {
        s = rest.to_owned();
    }
    if let Some(rest) = s.strip_suffix(".vmt") {
        s = rest.to_owned();
    }
    s
}

impl TexLights {
    pub fn parse(text: &str) -> Self {
        let mut end = text.len().min(MAX_RAD_BYTES);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        let text = text.get(..end).unwrap_or("");
        let mut plain: HashMap<String, Vec3> = HashMap::new();
        let mut ldr: HashMap<String, Vec3> = HashMap::new();
        let mut hdr: HashMap<String, Vec3> = HashMap::new();
        for line in text.lines() {
            let line = line.split("//").next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (target, rest) = if let Some(r) = strip_prefix_ci(line, "ldr:") {
                (&mut ldr, r)
            } else if let Some(r) = strip_prefix_ci(line, "hdr:") {
                (&mut hdr, r)
            } else {
                (&mut plain, line)
            };
            let mut parts = rest.splitn(2, char::is_whitespace);
            let (Some(name), Some(value)) = (parts.next(), parts.next()) else { continue };
            let lower = name.to_ascii_lowercase();
            if lower == "noshadow" || lower == "forcetextureshadow" {
                continue;
            }
            let Some(v) = light_for_string(value.trim()) else { continue };
            if target.len() < MAX_ENTRIES {
                target.insert(normalize_material(name), v);
            }
        }
        let mut high = plain.clone();
        high.extend(hdr);
        plain.extend(ldr);
        Self { map: plain, hdr: high }
    }

    pub fn merge(&mut self, other: TexLights) {
        for (mine, theirs) in [(&mut self.map, other.map), (&mut self.hdr, other.hdr)] {
            for (k, v) in theirs {
                if mine.len() < MAX_ENTRIES || mine.contains_key(&k) {
                    mine.insert(k, v);
                }
            }
        }
    }

    pub fn get(&self, material: &str) -> Option<Vec3> {
        self.map.get(material).copied()
    }

    pub fn get_hdr(&self, material: &str) -> Option<Vec3> {
        self.hdr.get(material).copied()
    }

    pub fn value(&self, material: &str, hdr: bool) -> Option<Vec3> {
        if hdr { self.get_hdr(material) } else { self.get(material) }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty() && self.hdr.is_empty()
    }
}

pub fn read_rad_file(path: &std::path::Path) -> Option<TexLights> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_RAD_BYTES as u64 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    Some(TexLights::parse(&String::from_utf8_lossy(&bytes)))
}

pub fn load(fs: Option<&hp_fs::GameFs>, game_dir: Option<&std::path::Path>, extra: &[std::path::PathBuf]) -> TexLights {
    let mut out = TexLights::default();
    let from_fs = fs
        .and_then(|f| {
            f.read_with("lights.rad", |b| {
                (b.len() <= MAX_RAD_BYTES).then(|| TexLights::parse(&String::from_utf8_lossy(b)))
            })
        })
        .flatten();
    match from_fs {
        Some(t) => out.merge(t),
        None => {
            if let Some(t) = game_dir.and_then(|d| read_rad_file(&d.join("lights.rad"))) {
                out.merge(t);
            }
        }
    }
    for path in extra {
        if let Some(t) = read_rad_file(path) {
            out.merge(t);
        }
    }
    out
}

fn strip_prefix_ci<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let head = s.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix).then(|| s.get(prefix.len()..).unwrap_or("").trim_start())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rad_files() {
        let text = "// comment\n\
            lights/white001 255 255 255 200\n\
            LIGHTS\\Fluorescentcool001A 237 248 255 100 // trailing\n\
            noshadow models/props/foo\n\
            hdr:lights/white001 255 255 255 1000\n\
            ldr:lights/white002 10 20 30 40\n\
            lights/white002 255 255 255 255\n\
            garbage\n\
            lights/bad nope\n";
        let t = TexLights::parse(text);
        assert_eq!(t.len(), 3);
        assert!((t.get("lights/white001").unwrap().x - 200.0).abs() < 1e-3, "hdr line ignored");
        assert!((t.get_hdr("lights/white001").unwrap().x - 1000.0).abs() < 1e-3, "the hdr line wins in HDR");
        assert!((t.value("lights/white002", true).unwrap().x - 255.0).abs() < 1e-3, "ldr lines are left out in HDR");
        assert!(t.get_hdr("lights/fluorescentcool001a").is_some(), "plain lines count in HDR");
        assert!(t.get("lights/fluorescentcool001a").is_some(), "names are normalized");
        assert!(t.get("lights/white002").unwrap().x < 10.0);
        assert!(t.get("lights/bad").is_none());
        let mut a = TexLights::parse("x 255 255 255 1");
        a.merge(TexLights::parse("x 255 255 255 2"));
        assert!((a.get("x").unwrap().x - 2.0).abs() < 1e-4);
        assert_eq!(normalize_material("Materials/Foo/Bar.vmt"), "foo/bar");
        assert!(TexLights::parse("").is_empty());
    }

    #[test]
    fn loads_files_in_order() {
        let dir = std::env::temp_dir().join(format!("hp_light_rad_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("lights.rad"),
            "a 255 255 255 10
b 255 255 255 10
",
        )
        .unwrap();
        let map = dir.join("map.rad");
        std::fs::write(
            &map,
            "b 255 255 255 20
",
        )
        .unwrap();
        let t = load(None, Some(&dir), &[map, dir.join("missing.rad")]);
        assert!((t.get("a").unwrap().x - 10.0).abs() < 1e-3);
        assert!((t.get("b").unwrap().x - 20.0).abs() < 1e-3, "the map file wins");
        assert!(read_rad_file(&dir).is_none(), "folders are not files");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
