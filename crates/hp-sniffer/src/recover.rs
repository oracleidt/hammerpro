use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use hp_bsp::{Bsp, lump};

use crate::procmem::MemSource;
use crate::scan::{Coverage, LumpCoverage, ScanOptions, probes_from_bsp, scan_probes};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub origin: String,
    pub modified: Option<SystemTime>,
}

#[derive(Debug)]
pub struct FileAccess {
    allowed: bool,
    opens: AtomicUsize,
}

impl FileAccess {
    pub fn new(allowed: bool) -> FileAccess {
        FileAccess { allowed, opens: AtomicUsize::new(0) }
    }

    pub fn allowed() -> FileAccess {
        FileAccess::new(true)
    }

    pub fn forbidden() -> FileAccess {
        FileAccess::new(false)
    }

    pub fn is_allowed(&self) -> bool {
        self.allowed
    }

    pub fn opens(&self) -> usize {
        self.opens.load(Ordering::SeqCst)
    }

    fn record(&self) -> bool {
        if !self.allowed {
            return false;
        }
        self.opens.fetch_add(1, Ordering::SeqCst);
        true
    }
}

pub fn safe_map_name(name: &str) -> Option<String> {
    let t = name.trim().trim_end_matches(".bsp").to_ascii_lowercase();
    let ok = !t.is_empty()
        && t.len() <= 120
        && t.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+'));
    ok.then_some(t)
}

fn push_file(out: &mut Vec<Candidate>, path: PathBuf, origin: &str) {
    if out.iter().any(|c| c.path == path) {
        return;
    }
    if let Ok(meta) = std::fs::metadata(&path)
        && meta.is_file()
        && meta.len() > 1024
    {
        out.push(Candidate { path, origin: origin.to_owned(), modified: meta.modified().ok() });
    }
}

fn header_mentions(path: &Path, needle: &[u8], access: &FileAccess) -> bool {
    use std::io::Read;
    if !access.record() {
        return false;
    }
    let Ok(f) = std::fs::File::open(path) else { return false };
    let mut buf = Vec::new();
    if f.take(8 << 20).read_to_end(&mut buf).is_err() {
        return false;
    }
    let lower: Vec<u8> = buf.iter().map(|b| b.to_ascii_lowercase()).collect();
    memchr::memmem::find(&lower, needle).is_some()
}

pub fn gma_with_map(dir: &Path, map: &str, out: &mut Vec<PathBuf>, access: &FileAccess) {
    let needle = format!("maps/{map}.bsp").into_bytes();
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if p.is_dir() {
            gma_with_map(&p, map, out, access);
        } else if (ext == "gma" || ext == "bin") && header_mentions(&p, &needle, access) {
            out.push(p);
        }
    }
}

pub fn find_candidates(game_dir: &Path, map: &str, scratch: &Path, access: &FileAccess) -> Vec<Candidate> {
    let mut out = Vec::new();
    if !access.is_allowed() {
        return out;
    }
    let Some(map) = safe_map_name(map) else { return out };
    push_file(&mut out, game_dir.join("maps").join(format!("{map}.bsp")), "maps folder");
    push_file(&mut out, game_dir.join("download").join("maps").join(format!("{map}.bsp")), "download folder");
    if let Ok(rd) = std::fs::read_dir(game_dir.join("cache")) {
        let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        files.sort();
        for p in files {
            if p.extension().and_then(|x| x.to_str()).is_some_and(|x| x.eq_ignore_ascii_case("bsp")) {
                push_file(&mut out, p, "game cache");
            }
        }
    }
    if let Ok(rd) = std::fs::read_dir(game_dir.join("addons")) {
        for e in rd.flatten() {
            push_file(&mut out, e.path().join("maps").join(format!("{map}.bsp")), "addon");
        }
    }
    let mut roots = vec![game_dir.join("cache").join("workshop")];
    if let Some(steamapps) = game_dir.parent().and_then(Path::parent).and_then(Path::parent) {
        roots.push(steamapps.join("workshop").join("content").join("4000"));
    }
    let mut archives = Vec::new();
    for r in roots {
        gma_with_map(&r, &map, &mut archives, access);
    }
    for (i, a) in archives.iter().enumerate() {
        if !access.record() {
            continue;
        }
        let Ok(gma) = hp_gma::GmaArchive::open(a) else { continue };
        let Ok(Some(data)) = gma.read(&format!("maps/{map}.bsp")) else { continue };
        let dest = scratch.join("from_gma").join(format!("{i}_{map}.bsp"));
        if let Some(dir) = dest.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if std::fs::write(&dest, data).is_ok() {
            let modified = std::fs::metadata(a).and_then(|m| m.modified()).ok();
            out.push(Candidate { path: dest, origin: format!("addon archive {}", a.display()), modified });
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct Evaluated {
    pub candidate: Candidate,
    pub entities_identical: bool,
    pub file_entities: usize,
    pub coverage: Vec<LumpCoverage>,
    pub verified_bytes: u64,
    pub verified_lumps: usize,
    pub rejected: Option<String>,
}

pub fn same_entities(memory: &str, file: &str) -> bool {
    hp_bsp::parse_entities(memory) == hp_bsp::parse_entities(file)
}

pub fn evaluate<S: MemSource + ?Sized>(
    src: &S,
    candidate: &Candidate,
    memory_entities: Option<&str>,
    opts: &ScanOptions,
    access: &FileAccess,
) -> (Evaluated, Option<Bsp>) {
    let mut ev = Evaluated {
        candidate: candidate.clone(),
        entities_identical: false,
        file_entities: 0,
        coverage: Vec::new(),
        verified_bytes: 0,
        verified_lumps: 0,
        rejected: None,
    };
    if !access.record() {
        ev.rejected = Some("reading map files is forbidden by the settings".to_owned());
        return (ev, None);
    }
    let bsp = match Bsp::open(&candidate.path) {
        Ok(b) => b,
        Err(e) => {
            ev.rejected = Some(e.to_string());
            return (ev, None);
        }
    };
    if let Ok(text) = bsp.entity_text() {
        ev.file_entities = hp_bsp::parse_entities(&text).len();
        if let Some(mem) = memory_entities {
            ev.entities_identical = same_entities(mem, &text);
        }
    }
    let probes = probes_from_bsp(&bsp, &[lump::ENTITIES, lump::PAKFILE]);
    let reader = |i: usize| bsp.lump(i).ok().map(|c| c.into_owned());
    ev.coverage = scan_probes(src, &probes, &reader, opts);
    for c in &ev.coverage {
        if c.state == Coverage::Verified {
            ev.verified_lumps += 1;
            ev.verified_bytes += c.len as u64;
        }
    }
    (ev, Some(bsp))
}

pub fn ranking(ev: &Evaluated) -> (bool, u64, Option<SystemTime>) {
    (ev.entities_identical, ev.verified_bytes, ev.candidate.modified)
}

pub fn assemble(bsp: Bsp, memory_entities: Option<&str>) -> Result<(Bsp, bool), hp_bsp::BspError> {
    let Some(mem) = memory_entities else { return Ok((bsp, false)) };
    let same = bsp.entity_text().map(|t| same_entities(mem, &t)).unwrap_or(false);
    if same {
        return Ok((bsp, false));
    }
    let mut data = mem.as_bytes().to_vec();
    if data.last() != Some(&0) {
        data.push(0);
    }
    let mut edit = bsp.edit();
    edit.replace_lump(lump::ENTITIES, data)?;
    let bytes = edit.to_vec()?;
    Ok((Bsp::from_vec(bytes)?, true))
}

pub fn bsp_from_memory_entities(text: &str) -> Result<Bsp, hp_bsp::BspError> {
    let mut data = text.as_bytes().to_vec();
    data.push(0);
    Bsp::from_vec(hp_bsp::synthetic_bsp(20, &[(lump::ENTITIES, data)]))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapCounts {
    pub entities: usize,
    pub brushes: usize,
    pub brush_sides: usize,
    pub faces: usize,
    pub displacements: usize,
    pub planes: usize,
    pub vertexes: usize,
    pub texinfo: usize,
    pub materials: usize,
    pub nodraw_sides: usize,
    pub tool_sides: usize,
    pub sky_sides: usize,
    pub clip_sides: usize,
    pub trigger_sides: usize,
    pub overlays: usize,
    pub cubemaps: usize,
    pub static_props: usize,
    pub area_portals: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrossRow {
    pub name: String,
    pub memory: usize,
    pub file: usize,
}

pub fn cross_check(memory: &MapCounts, file: &MapCounts) -> Vec<CrossRow> {
    let row = |name: &str, m: usize, f: usize| CrossRow { name: name.to_owned(), memory: m, file: f };
    vec![
        row("entities", memory.entities, file.entities),
        row("brushes", memory.brushes, file.brushes),
        row("brush sides", memory.brush_sides, file.brush_sides),
        row("planes", memory.planes, file.planes),
        row("vertexes", memory.vertexes, file.vertexes),
        row("faces", memory.faces, file.faces),
        row("texinfo", memory.texinfo, file.texinfo),
        row("materials", memory.materials, file.materials),
        row("displacements", memory.displacements, file.displacements),
        row("static props", memory.static_props, file.static_props),
        row("overlays", memory.overlays, file.overlays),
        row("cubemaps", memory.cubemaps, file.cubemaps),
        row("area portals", memory.area_portals, file.area_portals),
        row("nodraw sides", memory.nodraw_sides, file.nodraw_sides),
        row("other tool sides", memory.tool_sides, file.tool_sides),
        row("skybox sides", memory.sky_sides, file.sky_sides),
        row("clip sides", memory.clip_sides, file.clip_sides),
        row("trigger sides", memory.trigger_sides, file.trigger_sides),
    ]
}

pub fn map_counts(bsp: &Bsp) -> MapCounts {
    let len = |i: usize| bsp.lump_info(i).map(|l| l.uncompressed_len().unwrap_or(l.len) as usize).unwrap_or(0);
    let mut c = MapCounts {
        entities: bsp.entities().map(|e| e.len()).unwrap_or(0),
        brushes: len(18) / 12,
        brush_sides: len(19) / 8,
        faces: len(7) / 56,
        displacements: len(26) / 176,
        planes: len(1) / 20,
        vertexes: len(3) / 12,
        texinfo: len(6) / 72,
        overlays: len(45) / 352,
        cubemaps: len(42) / 16,
        area_portals: len(21) / 12,
        ..MapCounts::default()
    };
    c.static_props = bsp
        .game_lumps()
        .ok()
        .and_then(|g| {
            let lump = g.get(hp_bsp::gamelump::id::STATIC_PROPS)?;
            let data = lump.data().ok()?;
            hp_bsp::staticprops::StaticPropLump::parse(&data, lump.version, None).ok().map(|(l, _)| l.props.len())
        })
        .unwrap_or(0);
    let names = bsp.texture_names().unwrap_or_default();
    c.materials = names.len();
    let (Ok(sides), Ok(texinfo), Ok(texdata)) = (bsp.lump(19), bsp.lump(6), bsp.lump(2)) else { return c };
    for s in sides.chunks_exact(8) {
        let ti = i16::from_le_bytes([s[2], s[3]]);
        if ti < 0 {
            c.nodraw_sides += 1;
            continue;
        }
        let Some(t) = texinfo.chunks_exact(72).nth(ti as usize) else { continue };
        let td = i32::from_le_bytes([t[68], t[69], t[70], t[71]]);
        let Some(d) = texdata.chunks_exact(32).nth(td.max(0) as usize) else { continue };
        let name_index = i32::from_le_bytes([d[12], d[13], d[14], d[15]]).max(0) as usize;
        let name = names.get(name_index).map(|n| n.to_ascii_lowercase()).unwrap_or_default();
        if name.ends_with("toolsnodraw") {
            c.nodraw_sides += 1;
        } else if name.ends_with("toolsskybox") || name.starts_with("skybox/") {
            c.sky_sides += 1;
        } else if name.contains("toolsclip") || name.contains("toolsplayerclip") || name.contains("toolsnpcclip") {
            c.clip_sides += 1;
        } else if name.contains("toolstrigger") {
            c.trigger_sides += 1;
        } else if name.starts_with("tools/") {
            c.tool_sides += 1;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    fn entity_text(n: usize) -> String {
        let mut t = String::from("{\n\"classname\" \"worldspawn\"\n\"skyname\" \"sky_day01_01\"\n}\n");
        for i in 0..n {
            t.push_str(&format!(
                "{{\n\"classname\" \"info_target\"\n\"targetname\" \"t{i}\"\n\"origin\" \"{i} 0 0\"\n}}\n"
            ));
        }
        t
    }

    fn noise(n: usize, seed: u32) -> Vec<u8> {
        let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(7);
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as u8 | 0x80
            })
            .collect()
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hp_sniffer_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).expect("dir");
        d
    }

    fn write_bsp(path: &Path, text: &str, vertexes: &[u8]) {
        let mut ents = text.as_bytes().to_vec();
        ents.push(0);
        let data = hp_bsp::synthetic_bsp(20, &[(lump::ENTITIES, ents), (3, vertexes.to_vec())]);
        std::fs::write(path, data).expect("write");
    }

    #[test]
    fn map_names_are_safe() {
        assert_eq!(safe_map_name("RP_Rockford_v2b.bsp"), Some("rp_rockford_v2b".to_owned()));
        assert_eq!(safe_map_name("../x"), None);
        assert_eq!(safe_map_name("a b"), None);
        assert_eq!(safe_map_name(""), None);
    }

    #[test]
    fn candidates_come_from_every_place_the_game_keeps_maps() {
        let game = scratch("cands");
        std::fs::create_dir_all(game.join("maps")).expect("dir");
        std::fs::create_dir_all(game.join("cache")).expect("dir");
        std::fs::create_dir_all(game.join("download").join("maps")).expect("dir");
        let verts = noise(1200, 1);
        write_bsp(&game.join("maps").join("gm_x.bsp"), &entity_text(3), &verts);
        write_bsp(&game.join("cache").join("map_pack.bsp"), &entity_text(3), &verts);
        write_bsp(&game.join("download").join("maps").join("gm_x.bsp"), &entity_text(3), &verts);
        std::fs::write(game.join("cache").join("notes.txt"), b"x").expect("write");
        let found = find_candidates(&game, "gm_x", &game.join("scratch"), &FileAccess::allowed());
        let origins: Vec<&str> = found.iter().map(|c| c.origin.as_str()).collect();
        assert_eq!(found.len(), 3, "{origins:?}");
        assert!(
            origins.contains(&"game cache") && origins.contains(&"maps folder") && origins.contains(&"download folder")
        );
        assert!(find_candidates(&game, "../etc", &game.join("scratch"), &FileAccess::allowed()).is_empty());
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn the_file_that_matches_memory_wins() {
        let game = scratch("pick");
        std::fs::create_dir_all(game.join("maps")).expect("dir");
        std::fs::create_dir_all(game.join("cache")).expect("dir");
        let verts = noise(1200, 2);
        let other_verts = noise(1200, 3);
        let live = entity_text(30);
        write_bsp(&game.join("maps").join("gm_x.bsp"), &entity_text(5), &other_verts);
        write_bsp(&game.join("cache").join("map_pack.bsp"), &live, &verts);

        let mut mem = VecMem::default();
        let mut block = noise(400, 4);
        block.push(0);
        block.extend_from_slice(live.as_bytes());
        block.push(0);
        while !block.len().is_multiple_of(4) {
            block.push(0);
        }
        let mut b2 = noise(800, 5);
        let at = b2.len() as u64;
        b2.extend_from_slice(&verts);
        mem.add(0x10_0000, block);
        mem.add(0x40_0000, b2);

        let found = find_candidates(&game, "gm_x", &game.join("scratch"), &FileAccess::allowed());
        let opts = ScanOptions::default();
        let mut evs: Vec<Evaluated> =
            found.iter().map(|c| evaluate(&mem, c, Some(&live), &opts, &FileAccess::allowed()).0).collect();
        evs.sort_by_key(|e| std::cmp::Reverse(ranking(e)));
        assert_eq!(evs[0].candidate.origin, "game cache");
        assert!(evs[0].entities_identical);
        assert_eq!(evs[0].verified_lumps, 1);
        assert_eq!(evs[0].verified_bytes, verts.len() as u64);
        assert_eq!(evs[0].coverage[0].base, Some(0x40_0000 + at));
        assert!(!evs[1].entities_identical);
        assert_eq!(evs[1].verified_lumps, 0);
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn memory_entities_replace_the_files_when_they_differ() {
        let game = scratch("assemble");
        let path = game.join("a.bsp");
        let verts = noise(600, 6);
        write_bsp(&path, &entity_text(2), &verts);
        let bsp = Bsp::open(&path).expect("open");
        let live = entity_text(9);
        let (out, replaced) = assemble(bsp, Some(&live)).expect("assemble");
        assert!(replaced);
        assert_eq!(hp_bsp::parse_entities(&out.entity_text().expect("text")).len(), 10);
        assert_eq!(out.lump(3).expect("vertexes").as_ref(), verts.as_slice());
        let (same, replaced) = assemble(out, Some(&live)).expect("assemble");
        assert!(!replaced);
        assert_eq!(same.entities().expect("entities").len(), 10);
        let (untouched, replaced) = assemble(same, None).expect("assemble");
        assert!(!replaced);
        assert_eq!(untouched.entities().expect("entities").len(), 10);
        let _ = std::fs::remove_dir_all(&game);
    }

    #[test]
    fn a_map_with_only_entities_is_a_valid_bsp() {
        let bsp = bsp_from_memory_entities(&entity_text(4)).expect("bsp");
        assert_eq!(bsp.entities().expect("entities").len(), 5);
        let counts = map_counts(&bsp);
        assert_eq!(counts.entities, 5);
        assert_eq!(counts.brushes, 0);
    }
}
