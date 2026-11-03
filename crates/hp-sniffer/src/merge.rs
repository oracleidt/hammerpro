use std::collections::{HashMap, HashSet};

use hp_bsp::Bsp;
use hp_bsp::staticprops::StaticPropLump;

use crate::engine::FileDisp;
use crate::memmap::{MemoryWorld, plane_key};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MergeStats {
    pub file_used: bool,
    pub note: String,
    pub displacements: usize,
    pub displacements_from_file: usize,
    pub displacements_rejected: usize,
    pub props: usize,
    pub props_from_file: usize,
    pub props_added_from_file: usize,
    pub cubemaps: usize,
    pub cubemaps_from_file: usize,
    pub overlays: usize,
    pub overlays_from_file: usize,
    pub sides: usize,
    pub sides_from_file: usize,
    pub leafs_dropped: usize,
    pub texinfo: usize,
    pub texinfo_from_file: usize,
    pub pak_files: usize,
    pub disp_alpha_mem: usize,
    pub disp_allowed_mem: usize,
    pub prop_fade_mem: usize,
    pub prop_flags_mem: usize,
    pub cube_sizes_mem: usize,
    pub overlay_order_mem: usize,
    pub occluders_mem: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceRow {
    pub category: String,
    pub source: String,
    pub detail: String,
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn i16_at(d: &[u8], at: usize) -> i16 {
    i16::from_le_bytes([d[at], d[at + 1]])
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn lump(file: &Bsp, i: usize) -> Vec<u8> {
    file.lump(i).map(|c| c.into_owned()).unwrap_or_default()
}

fn merge_displacements(world: &mut MemoryWorld, file: &Bsp, stats: &mut MergeStats) {
    let faces = lump(file, 7);
    let infos = lump(file, 26);
    let verts = lump(file, 33);
    let tris = lump(file, 48);
    stats.displacements = world.extras.displacements.len();
    let Some(render) = &world.render else { return };
    if faces.len() / 56 != render.faces.len() || infos.is_empty() {
        return;
    }
    for d in &mut world.extras.displacements {
        let Some(rec) = faces.get(d.face * 56..d.face * 56 + 56) else { continue };
        let k = i16_at(rec, 12);
        if k < 0 {
            continue;
        }
        let Some(info) = infos.get(k as usize * 176..k as usize * 176 + 176) else { continue };
        let power = i32_at(info, 20);
        if power != d.power as i32 {
            continue;
        }
        let start = [f32_at(info, 0), f32_at(info, 4), f32_at(info, 8)];
        if start.iter().zip(&d.start).any(|(a, b)| (a - b).abs() > 0.5) {
            continue;
        }
        let row = (1usize << power) + 1;
        let (vs, ts) = (i32_at(info, 12), i32_at(info, 16));
        if vs < 0 || ts < 0 {
            continue;
        }
        let (vs, ts) = (vs as usize, ts as usize);
        let Some(vraw) = verts.get(vs * 20..(vs + row * row) * 20) else { continue };
        let tcount = 2 * (row - 1) * (row - 1);
        let Some(traw) = tris.get(ts * 2..(ts + tcount) * 2) else { continue };
        let mut allowed = [0u32; 10];
        for (n, a) in allowed.iter_mut().enumerate() {
            *a = i32_at(info, 136 + n * 4) as u32;
        }
        d.start = start;
        d.file = Some(FileDisp {
            min_tess: i32_at(info, 24),
            allowed,
            verts: vraw
                .chunks_exact(20)
                .map(|c| [f32_at(c, 0), f32_at(c, 4), f32_at(c, 8), f32_at(c, 12), f32_at(c, 16)])
                .collect(),
            tags: traw.chunks_exact(2).map(|c| u16_at(c, 0)).collect(),
        });
    }
}

fn norm_model(name: &str) -> String {
    name.replace(char::from(92u8), "/").to_ascii_lowercase()
}

fn merge_props(world: &mut MemoryWorld, file: &Bsp, stats: &mut MergeStats) {
    stats.props = world.extras.props.len();
    let Ok(game) = file.game_lumps() else { return };
    let Some(g) = game.get(hp_bsp::gamelump::id::STATIC_PROPS) else { return };
    let Ok(data) = g.data() else { return };
    let Ok((lump, _)) = StaticPropLump::parse(&data, g.version, None) else { return };
    let mut by_key: HashMap<(Vec<u32>, String), Vec<usize>> = HashMap::new();
    for (i, p) in lump.props.iter().enumerate() {
        let name = lump.names.get(usize::from(p.prop_type)).map(|n| norm_model(n)).unwrap_or_default();
        let key: Vec<u32> = p.origin.iter().chain(&p.angles).map(|v| v.to_bits()).collect();
        by_key.entry((key, name)).or_default().push(i);
    }
    for p in &mut world.extras.props {
        let key: Vec<u32> = p.origin.iter().chain(&p.angles).map(|v| v.to_bits()).collect();
        let Some(list) = by_key.get_mut(&(key, norm_model(&p.model))) else { continue };
        if list.is_empty() {
            continue;
        }
        let i = list.remove(0);
        p.file = Some(lump.props[i].clone());
    }
    let matched = world.extras.props.iter().filter(|p| p.file.is_some()).count();
    if matched == 0 || matched * 2 < world.extras.props.len() {
        return;
    }
    let mut left: Vec<usize> = by_key.into_values().flatten().collect();
    left.sort_unstable();
    for i in left {
        let rec = &lump.props[i];
        let Some(model) = lump.names.get(usize::from(rec.prop_type)) else { continue };
        world.extras.props.push(crate::engine::MemProp {
            origin: rec.origin,
            angles: rec.angles,
            model: model.clone(),
            alpha: 255,
            solid: rec.solid,
            skin: rec.skin.clamp(0, 255) as u8,
            fades: rec.fade_max > 0.0,
            first_leaf: rec.first_leaf,
            leaf_count: rec.leaf_count,
            file: Some(rec.clone()),
            ..Default::default()
        });
        stats.props_added_from_file += 1;
    }
}

fn merge_cubemaps(world: &mut MemoryWorld, file: &Bsp, stats: &mut MergeStats) {
    stats.cubemaps = world.extras.cubemaps.len();
    if world.extras.cubemap_file.is_some() {
        return;
    }
    let raw = lump(file, 42);
    let entries: Vec<[i32; 4]> =
        raw.chunks_exact(16).map(|c| [i32_at(c, 0), i32_at(c, 4), i32_at(c, 8), i32_at(c, 12)]).collect();
    if entries.is_empty() {
        return;
    }
    let known: HashSet<[i32; 3]> = entries.iter().map(|e| [e[0], e[1], e[2]]).collect();
    let matched = world.extras.cubemaps.iter().filter(|c| known.contains(*c)).count();
    if matched == 0 {
        return;
    }
    if matched == world.extras.cubemaps.len() {
        stats.cubemaps_from_file = entries.len();
        world.extras.cubemap_file = Some(entries);
    } else {
        let sizes: HashMap<[i32; 3], i32> = entries.iter().map(|e| ([e[0], e[1], e[2]], e[3])).collect();
        let list: Vec<[i32; 4]> = world
            .extras
            .cubemaps
            .iter()
            .map(|c| [c[0], c[1], c[2], sizes.get(c).copied().unwrap_or(crate::engine::CUBEMAP_DEFAULT_SIZE)])
            .collect();
        stats.cubemaps_from_file = matched;
        world.extras.cubemap_file = Some(list);
    }
}

fn merge_overlays(world: &mut MemoryWorld, file: &Bsp, stats: &mut MergeStats) {
    stats.overlays = world.extras.overlays.len();
    let raw = lump(file, 45);
    for o in &mut world.extras.overlays {
        let Some(rec) = raw.get(o.id as usize * 352..o.id as usize * 352 + 352) else { continue };
        let same_origin = (0..3).all(|k| (f32_at(rec, 328 + k * 4) - o.origin[k]).abs() < 0.01);
        if i32_at(rec, 0) == o.id && i16_at(rec, 4) as i32 == o.texinfo && same_origin {
            o.from_file = true;
            stats.overlays_from_file += 1;
        }
    }
}

fn merge_sides(world: &mut MemoryWorld, file: &Bsp, stats: &mut MergeStats) {
    let cm = &world.cm.cm;
    let brushes = lump(file, 18);
    let sides = lump(file, 19);
    let planes = lump(file, 1);
    let texinfo = lump(file, 6);
    let Some(render) = &world.render else { return };
    stats.sides = cm.total_sides();
    if brushes.len() / 12 != cm.brushes.len() || planes.is_empty() || texinfo.is_empty() {
        return;
    }
    let mut map: HashMap<(usize, ([i32; 3], i32)), i16> = HashMap::new();
    for b in 0..cm.brushes.len() {
        let (first, count) = (i32_at(&brushes, b * 12), i32_at(&brushes, b * 12 + 4));
        let memory_sides = if cm.brushes[b].axial() { 6 } else { cm.brushes[b].numsides };
        let count = if count == 0 && cm.brushes[b].axial() { memory_sides } else { count };
        if first < 0 || count != memory_sides {
            continue;
        }
        for k in first as usize..(first + count) as usize {
            let Some(s) = sides.get(k * 8..k * 8 + 8) else { break };
            let (plane, t) = (u16_at(s, 0) as usize, i16_at(s, 2));
            let Some(p) = planes.get(plane * 20..plane * 20 + 16) else { continue };
            let Some(mem) = render.texinfo.get(t.max(0) as usize) else { continue };
            if t < 0 {
                continue;
            }
            let same_axes = (0..16).all(|n| f32_at(&texinfo, t as usize * 72 + n * 4) == mem.vecs[n]);
            if !same_axes {
                continue;
            }
            let key = plane_key([f32_at(p, 0), f32_at(p, 4), f32_at(p, 8)], f32_at(p, 12));
            map.insert((b, key), t);
        }
    }
    stats.sides_from_file = map.len();
    if !map.is_empty() {
        world.extras.side_texinfo = Some(map);
    }
}

fn merge_pak(world: &mut MemoryWorld, file: &Bsp, stats: &mut MergeStats) {
    let raw = lump(file, hp_bsp::lump::PAKFILE);
    if raw.is_empty() {
        return;
    }
    if let Ok(pak) = file.pakfile() {
        stats.pak_files = pak.entries().len();
        if stats.pak_files > 0 {
            world.extras.pakfile = Some(raw);
        }
    }
}

const LEAF_RECORD: usize = 32;
const LEAF_ALIGN_MAX: usize = 4;

fn file_leaf_keys(file: &Bsp) -> Vec<(i32, i16, u16, u16, u16)> {
    lump(file, hp_bsp::lump::LEAFS)
        .chunks_exact(LEAF_RECORD)
        .map(|l| (i32_at(l, 0), i16_at(l, 4), u16_at(l, 6), u16_at(l, 24), u16_at(l, 26)))
        .collect()
}

fn leaf_key(l: &crate::memmap::CLeaf) -> (i32, i16, u16, u16, u16) {
    (l.contents, l.cluster, l.area_flags, l.first_brush, l.num_brushes)
}

pub fn leaf_offset(ours: &[crate::memmap::CLeaf], file: &[(i32, i16, u16, u16, u16)]) -> Option<usize> {
    if file.is_empty() || ours.len() <= file.len() {
        return None;
    }
    let extra = ours.len() - file.len();
    if extra > LEAF_ALIGN_MAX {
        return None;
    }
    (1..=extra).find(|d| file.iter().enumerate().all(|(i, f)| ours.get(i + d).map(leaf_key).as_ref() == Some(f)))
}

fn align_leafs(world: &mut MemoryWorld, file: &Bsp, stats: &mut MergeStats) {
    let keys = file_leaf_keys(file);
    let Some(d) = leaf_offset(&world.cm.leafs, &keys) else { return };
    world.cm.leafs.drain(..d);
    world.cm.leafs_addr += (d * crate::memmap::LEAF_RECORD) as u64;
    stats.leafs_dropped = d;
}

fn file_material_names(file: &Bsp) -> Vec<String> {
    let texinfo = lump(file, 6);
    let texdata = lump(file, hp_bsp::lump::TEXDATA);
    let names = file.texture_names().unwrap_or_default();
    texinfo
        .chunks_exact(72)
        .map(|t| {
            usize::try_from(i32_at(t, 68))
                .ok()
                .and_then(|d| texdata.get(d * 32 + 12..d * 32 + 16))
                .and_then(|b| usize::try_from(i32_at(b, 0)).ok())
                .and_then(|id| names.get(id))
                .cloned()
                .unwrap_or_default()
        })
        .collect()
}

fn texinfo_key(vecs: &[f32; 16], flags: i32) -> ([u32; 16], i32) {
    let mut bits = [0u32; 16];
    for (b, v) in bits.iter_mut().zip(vecs) {
        *b = v.to_bits();
    }
    (bits, flags)
}

fn merge_texinfo_names(world: &mut MemoryWorld, file: &Bsp, stats: &mut MergeStats) {
    let Some(render) = &world.render else { return };
    stats.texinfo = render.texinfo.len();
    let surfaces = &world.cm.cm.surface_names;
    if surfaces.is_empty() || render.texinfo.is_empty() {
        return;
    }
    let file_names = file_material_names(file);
    let texinfo = lump(file, 6);
    let mut by_key: HashMap<([u32; 16], i32), Option<String>> = HashMap::new();
    for (rec, name) in texinfo.chunks_exact(72).zip(&file_names) {
        let mut vecs = [0f32; 16];
        for (k, v) in vecs.iter_mut().enumerate() {
            *v = f32_at(rec, k * 4);
        }
        let key = texinfo_key(&vecs, i32_at(rec, 64));
        match by_key.get(&key) {
            Some(Some(seen)) if seen != name => {
                by_key.insert(key, None);
            }
            Some(_) => {}
            None => {
                by_key.insert(key, Some(name.clone()));
            }
        }
    }
    let mut index: HashMap<&str, i32> = HashMap::new();
    for (i, n) in surfaces.iter().enumerate() {
        index.entry(n.as_str()).or_insert(i as i32);
    }
    let mut ranks: Vec<i32> = Vec::with_capacity(render.texinfo.len());
    let mut found = 0usize;
    for t in &render.texinfo {
        let rank = by_key
            .get(&texinfo_key(&t.vecs, t.flags))
            .and_then(|n| n.as_deref())
            .and_then(|n| index.get(n).copied())
            .unwrap_or(-1);
        if rank >= 0 {
            found += 1;
        }
        ranks.push(rank);
    }
    if found == 0 {
        return;
    }
    stats.texinfo_from_file = found;
    if let Some(r) = world.render.as_mut() {
        r.texinfo_file_ranks = ranks;
    }
}

pub fn note_memory(world: &MemoryWorld, stats: &mut MergeStats) {
    let x = &world.extras;
    stats.displacements = x.displacements.len();
    stats.disp_alpha_mem = x.displacements.iter().filter(|d| d.alphas.is_some()).count();
    stats.disp_allowed_mem = x.displacements.iter().filter(|d| d.allowed.is_some()).count();
    stats.props = x.props.len();
    stats.prop_fade_mem = x.fades_set;
    stats.prop_flags_mem = x.props.iter().filter(|p| p.index.is_some()).count();
    stats.cubemaps = x.cubemaps.len();
    stats.cube_sizes_mem = x.cubemap_file.as_ref().map_or(0, Vec::len);
    stats.overlays = x.overlays.len();
    stats.overlay_order_mem = x.overlays.len();
    stats.occluders_mem = x.occlusion.as_ref().map_or(0, |o| o.occluders.len());
}

pub fn merge(world: &mut MemoryWorld, file: &Bsp) -> MergeStats {
    let mut stats = MergeStats { file_used: true, ..MergeStats::default() };
    note_memory(world, &mut stats);
    align_leafs(world, file, &mut stats);
    merge_texinfo_names(world, file, &mut stats);
    merge_displacements(world, file, &mut stats);
    merge_props(world, file, &mut stats);
    merge_cubemaps(world, file, &mut stats);
    merge_overlays(world, file, &mut stats);
    merge_sides(world, file, &mut stats);
    merge_pak(world, file, &mut stats);
    stats.props = world.extras.props.len();
    stats.props_from_file = world.extras.props.iter().filter(|p| p.file.is_some()).count();
    stats.displacements_from_file = world.extras.displacements.iter().filter(|d| d.file.is_some()).count();
    stats
}

fn texinfo_row(stats: &MergeStats) -> SourceRow {
    let row = |s: &str, d: String| SourceRow {
        category: "faces and texture axes".to_owned(),
        source: s.to_owned(),
        detail: d,
    };
    if stats.texinfo_from_file >= stats.texinfo && stats.texinfo > 0 {
        row("memory and map file", format!("the material of all {} texture infos confirmed by the file", stats.texinfo))
    } else if stats.texinfo_from_file > 0 {
        row(
            "memory and map file",
            format!("the material of {} of {} texture infos from the file", stats.texinfo_from_file, stats.texinfo),
        )
    } else {
        row("memory", String::new())
    }
}

pub fn source_rows(stats: &MergeStats, file_allowed: bool, game_files: usize, memory_models: usize) -> Vec<SourceRow> {
    let row = |c: &str, s: &str, d: String| SourceRow { category: c.to_owned(), source: s.to_owned(), detail: d };
    let file = stats.file_used;
    let pick = |from_file: usize, total: usize| -> (&'static str, String) {
        if file && from_file > 0 {
            if from_file >= total {
                ("map file", format!("{from_file} of {total}"))
            } else {
                ("memory and map file", format!("{from_file} of {total} from the file"))
            }
        } else {
            ("absent", format!("0 of {total}"))
        }
    };
    let pick_mem = |mem: usize, from_file: usize, total: usize| -> (&'static str, String) {
        if total > 0 && mem >= total {
            ("memory", format!("{mem} of {total}"))
        } else if mem > 0 && file && from_file > 0 {
            ("memory and map file", format!("{mem} of {total} from the memory, the rest from the file"))
        } else if mem > 0 {
            ("memory", format!("{mem} of {total}"))
        } else {
            pick(from_file, total)
        }
    };
    let mut out = vec![
        row("entities", "memory", String::new()),
        row("brushes, planes and materials", "memory", String::new()),
        texinfo_row(stats),
    ];
    let (s, d) = if file {
        ("memory and map file", format!("{} of {} sides from the file", stats.sides_from_file, stats.sides))
    } else {
        ("memory", "sides no face covers take the texture info of the same material that fits their plane (approximate)".to_owned())
    };
    out.push(row("texture axes of hidden sides", s, d));
    out.push(row("displacement geometry", "memory", String::new()));
    let (s, d) = pick_mem(stats.disp_alpha_mem, stats.displacements_from_file, stats.displacements);
    out.push(row("displacement vertex alphas", s, d));
    let (s, d) = pick_mem(stats.disp_allowed_mem, stats.displacements_from_file, stats.displacements);
    out.push(row("displacement allowed vertices", s, d));
    out.push(row("displacement triangle tags", "memory", "computed from the slopes of the recovered geometry".to_owned()));
    if stats.props_added_from_file > 0 {
        out.push(row(
            "static props placement",
            "memory and map file",
            format!("{} the engine did not create (model not found) come from the file", stats.props_added_from_file),
        ));
    } else {
        out.push(row("static props placement", "memory", String::new()));
    }
    let (s, d) = pick_mem(stats.prop_flags_mem, stats.props_from_file, stats.props);
    out.push(row("static props flags", s, d));
    let (s, d) = if stats.prop_fade_mem > 0 {
        ("memory", format!("{} fade records", stats.prop_fade_mem))
    } else {
        pick(stats.props_from_file, stats.props)
    };
    out.push(row("static props fade distances", s, d));
    out.push(row("overlays", "memory", String::new()));
    let (s, d) = pick_mem(stats.overlay_order_mem, stats.overlays_from_file, stats.overlays);
    out.push(row("overlay render order", s, d));
    out.push(row("cubemaps", "memory", String::new()));
    let (s, d) = pick_mem(stats.cube_sizes_mem, stats.cubemaps_from_file, stats.cubemaps);
    out.push(row("cubemap sizes", s, d));
    if stats.occluders_mem > 0 {
        out.push(row("occluders", "memory", format!("{} occluders", stats.occluders_mem)));
    } else {
        out.push(row("occluders", "memory", "none in the engine".to_owned()));
    }
    out.push(row("area portals", "memory", String::new()));
    let pak = if stats.pak_files > 0 {
        row("pakfile", "map file", format!("{} files", stats.pak_files))
    } else if !file_allowed {
        row("pakfile", "absent", "map file reads are forbidden".to_owned())
    } else {
        row("pakfile", "absent", "no readable map file matched the memory".to_owned())
    };
    out.push(pak);
    let content = if game_files > 0 || memory_models > 0 {
        row(
            "custom and stock content",
            "game content",
            format!("{game_files} files on disk, {memory_models} models from memory"),
        )
    } else {
        row("custom and stock content", "absent", String::new())
    };
    out.push(content);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(contents: i32, cluster: i16, first_brush: u16, num_brushes: u16) -> crate::memmap::CLeaf {
        crate::memmap::CLeaf { contents, cluster, area_flags: 0x400, first_brush, num_brushes }
    }

    #[test]
    fn a_false_leaf_in_front_of_the_leaf_list_is_found_and_dropped() {
        let real: Vec<crate::memmap::CLeaf> = (0..40).map(|i| leaf(1, i as i16 - 1, i as u16, 1)).collect();
        let keys: Vec<(i32, i16, u16, u16, u16)> = real.iter().map(leaf_key).collect();
        assert_eq!(leaf_offset(&real, &keys), None, "an aligned list needs no change");
        let mut shifted = vec![leaf(0x3663_3934, 50, 0, 0)];
        shifted.extend(real.clone());
        assert_eq!(leaf_offset(&shifted, &keys), Some(1));
        let mut two = vec![leaf(7, 3, 0, 0), leaf(9, 4, 0, 0)];
        two.extend(real.clone());
        assert_eq!(leaf_offset(&two, &keys), Some(2));
        let mut middle = real.clone();
        middle.insert(20, leaf(0x3663_3934, 50, 0, 0));
        assert_eq!(leaf_offset(&middle, &keys), None, "an extra leaf inside the list is not an offset");
        let mut far = vec![leaf(1, 0, 0, 0); 9];
        far.extend(real.clone());
        assert_eq!(leaf_offset(&far, &keys), None, "too many extra leaves is not an offset");
    }
}
