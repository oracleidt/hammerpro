use crate::header::lump;
use crate::layout::{LayoutOptions, relayout};
use crate::reorder::{ReorderOptions, reorder};
use crate::rng::{Rng, entropy};
use crate::{Bsp, Entity, Result};

pub const LOGIC_CLASSES: &[&str] = &[
    "logic_",
    "math_",
    "filter_",
    "point_template",
    "point_servercommand",
    "point_clientcommand",
    "env_microphone",
    "env_entity_maker",
    "game_",
    "ai_",
    "npc_",
    "info_node",
    "info_target",
    "path_track",
    "path_corner",
    "scripted_",
    "phys_",
    "comp_",
    "trigger_",
    "func_instance",
    "weapon_",
    "item_",
];

const STRIP_KEYS: &[&str] = &["hammerid", "comment", "_comment", "parentname", "classname_original"];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProtectReport {
    pub entities_before: usize,
    pub entities_kept: usize,
    pub entities_removed: usize,
    pub connections_stripped: usize,
    pub original_faces_bytes: usize,
    pub materials_randomized: usize,
    pub custom_assets_renamed: usize,
    pub layout_seed: String,
    pub decoy_blocks: usize,
    pub decoy_bytes: usize,
    pub gap_bytes: usize,
    pub reordered: Vec<String>,
}

pub const LOCK_LUA_PATH: &str = "lua/autorun/hammerpro_lock.lua";
pub const LOCK_LUA: &str = include_str!("../../../assets/protect/hammerpro_lock.lua");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lock {
    pub hash: String,
    pub block: bool,
}

pub fn lock_hash(key: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in key.as_bytes() {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x0100_0193);
    }
    format!("{h:08x}")
}

#[derive(Clone, Debug)]
pub struct ProtectOptions {
    pub watermark: String,
    pub strip_logic: bool,
    pub strip_connections: bool,
    pub strip_original_faces: bool,
    pub extra_logic: Vec<String>,
    pub lock: Option<Lock>,
    pub randomize_materials: bool,
    pub salt: String,
    pub layout: LayoutOptions,
    pub reorder: ReorderOptions,
    pub layout_seed: String,
}

impl ProtectOptions {
    pub fn strongest() -> Self {
        Self {
            randomize_materials: true,
            layout: LayoutOptions::strongest(),
            reorder: ReorderOptions { tables: true, world: true },
            ..Self::default()
        }
    }
}

impl Default for ProtectOptions {
    fn default() -> Self {
        Self {
            watermark: String::new(),
            strip_logic: true,
            strip_connections: true,
            strip_original_faces: true,
            extra_logic: Vec::new(),
            lock: None,
            randomize_materials: false,
            salt: String::new(),
            layout: LayoutOptions::default(),
            reorder: ReorderOptions::default(),
            layout_seed: String::new(),
        }
    }
}

fn randomizable(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    !n.is_empty()
        && !n.starts_with("tools/")
        && !n.starts_with("maps/")
        && !n.starts_with("engine/")
        && !n.contains("_rt_")
        && n.len() < 240
}

fn garbage_name(original: &str, salt: &str, taken: &std::collections::HashSet<String>) -> String {
    let mut n = 0u32;
    loop {
        let h = lock_hash(&format!("{salt}/{original}#{n}"));
        let name = format!("hp/{h}");
        if !taken.contains(&name) {
            return name;
        }
        n += 1;
    }
}

const VMT_TEXTURE_KEYS: &[&str] = &[
    "$basetexture",
    "$basetexture2",
    "$basetexture3",
    "$basetexture4",
    "$bumpmap",
    "$bumpmap2",
    "$normalmap",
    "$detail",
    "$detail2",
    "$envmapmask",
    "$blendmodulatetexture",
    "$selfillummask",
    "$lightwarptexture",
    "$phongexponenttexture",
    "$phongwarptexture",
    "$ssbump",
    "$tooltexture",
    "$texture2",
    "$hdrbasetexture",
    "$bottommaterial",
];

fn rewrite_vmt(text: &str, rename: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        let key = VMT_TEXTURE_KEYS.iter().find(|k| lower.contains(*k));
        let rewritten = key.and_then(|_| {
            let mut parts = line.match_indices('"');
            let (_, _) = parts.next()?;
            let (_, _) = parts.next()?;
            let (start, _) = parts.next()?;
            let (end, _) = parts.next()?;
            let value = &line[start + 1..end];
            let new = rename(value)?;
            Some(format!("{}{}{}", &line[..start + 1], new, &line[end..]))
        });
        out.push_str(&rewritten.unwrap_or_else(|| line.to_owned()));
        out.push('\n');
    }
    out
}

fn patch_vmt(original: &str) -> Vec<u8> {
    format!("\"patch\"\n{{\n\t\"include\" \"materials/{original}.vmt\"\n}}\n").into_bytes()
}

struct Randomized {
    string_data: Vec<u8>,
    string_table: Vec<u8>,
    patches: Vec<(String, String)>,
    renamed_custom: usize,
}

fn randomize(bsp: &Bsp, salt: &str, pak: &mut crate::PakWriter) -> Result<Randomized> {
    let data = bsp.lump(lump::TEXDATA_STRING_DATA)?;
    let table = bsp.lump(lump::TEXDATA_STRING_TABLE)?;
    let name_at = |ofs: usize| -> String {
        let rest = data.get(ofs..).unwrap_or(&[]);
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len()).min(1024);
        String::from_utf8_lossy(&rest[..end]).into_owned()
    };
    let archive = bsp.pakfile()?;
    let mut taken: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut renames: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut patches: Vec<(String, String)> = Vec::new();
    let mut renamed_custom = 0usize;
    let mut texture_renames: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    let mut plan = |original: &str,
                    taken: &mut std::collections::HashSet<String>,
                    pak: &mut crate::PakWriter,
                    patches: &mut Vec<(String, String)>,
                    texture_renames: &mut std::collections::HashMap<String, String>|
     -> Result<String> {
        let generated = garbage_name(original, salt, taken);
        taken.insert(generated.clone());
        let vmt_entry = format!("materials/{}.vmt", original.to_ascii_lowercase());
        let Some(vmt) = archive.read_name(&vmt_entry)? else {
            patches.push((generated.clone(), original.to_owned()));
            return Ok(generated);
        };
        let text = String::from_utf8_lossy(&vmt).into_owned();
        let mut wanted: Vec<String> = Vec::new();
        for line in text.lines() {
            let lower = line.to_ascii_lowercase();
            if !VMT_TEXTURE_KEYS.iter().any(|k| lower.contains(*k)) {
                continue;
            }
            let mut q = line.match_indices('"').map(|(i, _)| i);
            if let (Some(_), Some(_), Some(s), Some(e)) = (q.next(), q.next(), q.next(), q.next()) {
                wanted.push(line[s + 1..e].trim().to_ascii_lowercase());
            }
        }
        for tex in wanted {
            if texture_renames.contains_key(&tex) {
                continue;
            }
            let entry = format!("materials/{tex}.vtf");
            let Some(bytes) = archive.read_name(&entry)? else { continue };
            let new_tex = garbage_name(&tex, salt, taken);
            taken.insert(new_tex.clone());
            pak.remove(&entry);
            pak.add(&format!("materials/{new_tex}.vtf"), bytes.into_owned())?;
            texture_renames.insert(tex, new_tex);
        }
        let new_text = rewrite_vmt(&text, &|v| texture_renames.get(&v.trim().to_ascii_lowercase()).cloned());
        pak.remove(&vmt_entry);
        pak.add(&format!("materials/{generated}.vmt"), new_text.into_bytes())?;
        renamed_custom += 1;
        Ok(generated)
    };

    let mut new_data: Vec<u8> = Vec::with_capacity(data.len());
    let mut offset_of: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut new_table: Vec<u8> = Vec::with_capacity(table.len());
    for c in table.chunks_exact(4) {
        let ofs = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize;
        let original = name_at(ofs);
        let new_name = if randomizable(&original) {
            match renames.get(&original) {
                Some(g) => g.clone(),
                None => {
                    let g = plan(&original, &mut taken, pak, &mut patches, &mut texture_renames)?;
                    renames.insert(original.clone(), g.clone());
                    g
                }
            }
        } else {
            original.clone()
        };
        let o = match offset_of.get(&new_name) {
            Some(o) => *o,
            None => {
                let o = new_data.len() as u32;
                new_data.extend_from_slice(new_name.as_bytes());
                new_data.push(0);
                offset_of.insert(new_name.clone(), o);
                o
            }
        };
        new_table.extend_from_slice(&o.to_le_bytes());
    }
    patches.sort();
    Ok(Randomized { string_data: new_data, string_table: new_table, patches, renamed_custom })
}

fn is_logic(class: &str, extra: &[String]) -> bool {
    let c = class.trim().to_ascii_lowercase();
    let hit = |p: &str| {
        if let Some(prefix) = p.strip_suffix('_') { c.starts_with(prefix) && c.contains('_') } else { c == p }
    };
    LOGIC_CLASSES.iter().any(|p| hit(p)) || extra.iter().any(|p| hit(p))
}

fn has_brush_model(e: &Entity) -> bool {
    e.get("model").is_some_and(|m| m.starts_with('*'))
}

fn is_connection(key: &str, value: &str) -> bool {
    let name = key.trim();
    let looks_output = name.len() > 2 && name[..2].eq_ignore_ascii_case("On");
    looks_output && (value.contains('\u{1b}') || value.matches(',').count() >= 3)
}

fn mark_world(e: &mut Entity, opts: &ProtectOptions) {
    if !opts.watermark.is_empty() {
        e.set("hammerpro_protected", opts.watermark.clone());
    }
    if let Some(lock) = &opts.lock {
        e.set("hammerpro_lock", lock.hash.clone());
        e.set("hammerpro_lock_block", if lock.block { "1" } else { "0" });
    }
}

pub fn server_copy(protected: &[u8], full: &Bsp, opts: &ProtectOptions) -> Result<Vec<u8>> {
    let mut entities = full.entities()?;
    for e in entities.iter_mut().filter(|e| e.classname().eq_ignore_ascii_case("worldspawn")) {
        mark_world(e, opts);
    }
    let mut edit = Bsp::from_vec(protected.to_vec())?.edit();
    edit.entities_last(true);
    edit.set_entities(&entities)?;
    edit.to_vec()
}

pub fn protect(full: &Bsp, opts: &ProtectOptions) -> Result<(Vec<u8>, ProtectReport)> {
    let mut entities = full.entities()?;
    let mut report = ProtectReport { entities_before: entities.len(), ..Default::default() };

    entities.retain_mut(|e| {
        let class = e.classname().to_ascii_lowercase();
        let is_world = class == "worldspawn";
        if opts.strip_logic && !is_world && !has_brush_model(e) && is_logic(&class, &opts.extra_logic) {
            report.entities_removed += 1;
            return false;
        }
        e.pairs.retain(|(k, v)| {
            if opts.strip_connections && is_connection(k, v) {
                report.connections_stripped += 1;
                return false;
            }
            !STRIP_KEYS.iter().any(|s| k.eq_ignore_ascii_case(s))
        });
        if is_world {
            mark_world(e, opts);
        }
        true
    });
    report.entities_kept = entities.len();

    let mut edit = Bsp::from_vec(full.bytes().to_vec())?.edit();
    edit.entities_last(true);
    edit.set_entities(&entities)?;
    let seed = if opts.layout_seed.trim().is_empty() {
        format!("{:016x}", entropy())
    } else {
        opts.layout_seed.trim().to_owned()
    };
    let mut rng = Rng::from_text(&seed);
    if opts.reorder.tables || opts.reorder.world || opts.layout.any() {
        report.layout_seed = seed.clone();
    }
    if opts.reorder.tables || opts.reorder.world {
        let (changed, rep) = reorder(full, opts.reorder, &mut rng)?;
        for (index, data) in changed {
            edit.replace_lump(index, data)?;
        }
        report.reordered = rep.applied.iter().map(|s| (*s).to_owned()).collect();
    }
    if opts.strip_original_faces {
        if let Ok(raw) = full.lump_raw(lump::ORIGINAL_FACES) {
            report.original_faces_bytes = raw.len();
        }
        edit.remove_lump(lump::ORIGINAL_FACES)?;
    }
    if opts.randomize_materials {
        let mut pak = full.pakfile()?.to_writer()?;
        let r = randomize(full, &opts.salt, &mut pak)?;
        edit.replace_lump(lump::TEXDATA_STRING_DATA, r.string_data)?;
        edit.replace_lump(lump::TEXDATA_STRING_TABLE, r.string_table)?;
        for (generated, original) in &r.patches {
            pak.add(&format!("materials/{generated}.vmt"), patch_vmt(original))?;
        }
        edit.set_pakfile(pak.finish(crate::PakOptions::default())?)?;
        report.materials_randomized = r.patches.len() + r.renamed_custom;
        report.custom_assets_renamed = r.renamed_custom;
    }
    let bytes = edit.to_vec()?;
    if !opts.layout.any() {
        return Ok((bytes, report));
    }
    let (bytes, layout) = relayout(&bytes, &opts.layout, &mut rng)?;
    report.decoy_blocks = layout.decoy_blocks;
    report.decoy_bytes = layout.decoy_bytes;
    report.gap_bytes = layout.gap_bytes;
    Ok((bytes, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ent(pairs: &[(&str, &str)]) -> Entity {
        Entity { pairs: pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
    }

    fn bsp_with(entities: &[Entity]) -> Bsp {
        let text = crate::entities::write_entities(entities);
        let bytes = crate::synthetic_bsp(20, &[(lump::ENTITIES, text), (lump::ORIGINAL_FACES, vec![1u8; 64])]);
        Bsp::from_vec(bytes).expect("bsp")
    }

    #[test]
    fn logic_goes_but_geometry_and_render_stay() {
        let bsp = bsp_with(&[
            ent(&[("classname", "worldspawn"), ("skyname", "sky_day01_01"), ("comment", "secret note")]),
            ent(&[("classname", "logic_auto"), ("OnMapSpawn", "relay,Trigger,,0,-1")]),
            ent(&[("classname", "math_counter"), ("targetname", "cnt")]),
            ent(&[("classname", "trigger_multiple"), ("model", "*1"), ("OnStartTouch", "door,Open,,0,-1")]),
            ent(&[("classname", "func_door"), ("model", "*2"), ("OnFullyOpen", "x,y,,0,-1"), ("speed", "100")]),
            ent(&[("classname", "env_cubemap"), ("origin", "0 0 0")]),
            ent(&[("classname", "prop_dynamic"), ("model", "models/a.mdl"), ("targetname", "p")]),
        ]);
        let (bytes, r) = protect(&bsp, &ProtectOptions { watermark: "author42".into(), ..Default::default() }).unwrap();
        let out = Bsp::from_vec(bytes).unwrap();
        let classes: Vec<String> = out.entities().unwrap().iter().map(|e| e.classname().to_owned()).collect();
        assert!(classes.contains(&"func_door".to_string()));
        assert!(classes.contains(&"trigger_multiple".to_string()), "a brush trigger keeps its geometry");
        assert!(classes.contains(&"env_cubemap".to_string()));
        assert!(classes.contains(&"worldspawn".to_string()));
        assert!(!classes.contains(&"logic_auto".to_string()));
        assert!(!classes.contains(&"math_counter".to_string()));
        assert!(classes.contains(&"prop_dynamic".to_string()));
        assert_eq!((r.entities_removed, r.entities_kept), (2, 5));
        let door = out.entities().unwrap().into_iter().find(|e| e.classname() == "func_door").unwrap();
        assert_eq!(door.get("speed"), Some("100"));
        assert!(door.get("OnFullyOpen").is_none(), "I/O stripped");
        assert!(r.connections_stripped >= 2);
        let world = out.entities().unwrap().into_iter().find(|e| e.classname() == "worldspawn").unwrap();
        assert_eq!(world.get("skyname"), Some("sky_day01_01"), "rendering kept");
        assert_eq!(world.get("comment"), None, "editor note dropped");
        assert_eq!(world.get("hammerpro_protected"), Some("author42"), "watermark");
        assert_eq!(r.original_faces_bytes, 64);
        assert!(out.lump_raw(lump::ORIGINAL_FACES).unwrap().is_empty());
    }

    #[test]
    fn options_are_honored() {
        let bsp = bsp_with(&[
            ent(&[("classname", "worldspawn")]),
            ent(&[("classname", "logic_auto"), ("OnMapSpawn", "a,b,,0,-1")]),
        ]);
        let keep = ProtectOptions {
            strip_logic: false,
            strip_connections: false,
            strip_original_faces: false,
            watermark: String::new(),
            extra_logic: Vec::new(),
            lock: None,
            randomize_materials: false,
            salt: String::new(),
            layout: LayoutOptions::default(),
            reorder: ReorderOptions::default(),
            layout_seed: String::new(),
        };
        let (bytes, r) = protect(&bsp, &keep).unwrap();
        assert_eq!((r.entities_removed, r.connections_stripped, r.original_faces_bytes), (0, 0, 0));
        let out = Bsp::from_vec(bytes).unwrap();
        assert_eq!(out.entities().unwrap().len(), 2);
        assert!(!out.lump_raw(lump::ORIGINAL_FACES).unwrap().is_empty());
    }

    #[test]
    fn the_lock_keys_and_hash() {
        assert_eq!(lock_hash("secret"), lock_hash("secret"));
        assert_ne!(lock_hash("secret"), lock_hash("other"));
        assert_eq!(lock_hash("").len(), 8);
        assert_eq!(lock_hash("secret"), "667c6911");
        let bsp = bsp_with(&[ent(&[("classname", "worldspawn")])]);
        let opts = ProtectOptions { lock: Some(Lock { hash: lock_hash("my key"), block: true }), ..Default::default() };
        let (bytes, _) = protect(&bsp, &opts).unwrap();
        let world = Bsp::from_vec(bytes).unwrap().entities().unwrap().into_iter().next().unwrap();
        assert_eq!(world.get("hammerpro_lock"), Some(lock_hash("my key").as_str()));
        assert_eq!(world.get("hammerpro_lock_block"), Some("1"));
    }

    #[test]
    fn material_randomizer_renames_and_patches() {
        let names = ["brick/wall", "tools/toolsnodraw", "nature/water_canals"];
        let mut data = Vec::new();
        let mut offsets = Vec::new();
        for n in names {
            offsets.push(data.len() as u32);
            data.extend_from_slice(n.as_bytes());
            data.push(0);
        }
        let mut table = Vec::new();
        for o in &offsets {
            table.extend_from_slice(&o.to_le_bytes());
        }
        let empty_zip = vec![0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let ents = crate::entities::write_entities(&[ent(&[("classname", "worldspawn")])]);
        let bytes = crate::synthetic_bsp(
            20,
            &[
                (lump::ENTITIES, ents),
                (lump::TEXDATA_STRING_DATA, data),
                (lump::TEXDATA_STRING_TABLE, table),
                (lump::PAKFILE, empty_zip),
            ],
        );
        let bsp = Bsp::from_vec(bytes).unwrap();
        let (out_bytes, r) =
            protect(&bsp, &ProtectOptions { randomize_materials: true, ..Default::default() }).unwrap();
        assert_eq!(r.materials_randomized, 2, "the tool material is kept");
        let out = Bsp::from_vec(out_bytes).unwrap();
        let out_names = out.texture_names().unwrap();
        assert!(out_names.contains(&"tools/toolsnodraw".to_string()));
        assert!(!out_names.contains(&"brick/wall".to_string()));
        assert!(!out_names.contains(&"nature/water_canals".to_string()));
        assert_eq!(out_names.iter().filter(|n| n.starts_with("hp/")).count(), 2);
        assert_eq!(out_names.len(), 3, "same number of entries, same order");
        let pak = out.pakfile().unwrap();
        let brick_patch = out_names.iter().find(|n| n.starts_with("hp/")).unwrap();
        let vmt = pak.read_name(&format!("materials/{brick_patch}.vmt")).unwrap().expect("patch vmt");
        let text = String::from_utf8_lossy(&vmt);
        assert!(text.contains("\"patch\"") && text.contains("\"include\""));
        assert!(text.contains("materials/brick/wall.vmt") || text.contains("materials/nature/water_canals.vmt"));
    }

    #[test]
    fn custom_assets_are_renamed_for_good() {
        let mut pak = crate::PakWriter::new();
        pak.add(
            "materials/custom/brick.vmt",
            b"\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"custom/brick_d\"\n}\n".to_vec(),
        )
        .unwrap();
        pak.add("materials/custom/brick_d.vtf", vec![7u8; 32]).unwrap();
        let zip = pak.finish(crate::PakOptions::default()).unwrap();
        let mut data = Vec::new();
        let mut table = Vec::new();
        for n in ["custom/brick", "brick/wall"] {
            table.extend_from_slice(&(data.len() as u32).to_le_bytes());
            data.extend_from_slice(n.as_bytes());
            data.push(0);
        }
        let ents = crate::entities::write_entities(&[ent(&[("classname", "worldspawn")])]);
        let bsp = Bsp::from_vec(crate::synthetic_bsp(
            20,
            &[
                (lump::ENTITIES, ents),
                (lump::TEXDATA_STRING_DATA, data),
                (lump::TEXDATA_STRING_TABLE, table),
                (lump::PAKFILE, zip),
            ],
        ))
        .unwrap();
        let opts = ProtectOptions { randomize_materials: true, salt: "s3cret".into(), ..Default::default() };
        let (bytes, r) = protect(&bsp, &opts).unwrap();
        assert_eq!((r.materials_randomized, r.custom_assets_renamed), (2, 1));
        let out = Bsp::from_vec(bytes).unwrap();
        let out_pak = out.pakfile().unwrap();
        let names: Vec<String> = out_pak.entries().iter().map(|e| e.name.to_ascii_lowercase()).collect();
        assert!(!names.iter().any(|n| n.contains("custom/brick")), "{names:?}");
        assert_eq!(names.iter().filter(|n| n.starts_with("materials/hp/")).count(), 3, "vmt, vtf and the game patch");
        let vmt = names.iter().find(|n| n.ends_with(".vmt") && !n.contains("brick")).unwrap();
        let text = String::from_utf8_lossy(&out_pak.read_name(vmt).unwrap().unwrap()).into_owned();
        if text.contains("$basetexture") {
            assert!(text.contains("hp/"), "texture reference rewritten: {text}");
            assert!(!text.contains("custom/brick_d"), "{text}");
        }
        let other = ProtectOptions { salt: "other".into(), ..opts.clone() };
        let (b2, _) = protect(&bsp, &other).unwrap();
        let n2 = Bsp::from_vec(b2).unwrap().texture_names().unwrap();
        assert_ne!(n2, out.texture_names().unwrap(), "a different salt gives different names");
    }

    #[test]
    fn randomizable_filter() {
        assert!(randomizable("brick/wall"));
        assert!(!randomizable("tools/toolsnodraw"));
        assert!(!randomizable("maps/gm_x/brick/wall_1_2_3"));
        assert!(!randomizable(""));
    }

    #[test]
    fn extra_logic_classes() {
        assert!(is_logic("logic_relay", &[]));
        assert!(is_logic("npc_zombie", &[]));
        assert!(is_logic("trigger_hurt", &[]));
        assert!(!is_logic("func_door", &[]));
        assert!(!is_logic("env_cubemap", &[]));
        assert!(is_logic("my_secret_brain", &["my_secret_".to_owned()]));
    }

    #[test]
    fn server_copy_differs_only_by_its_entities() {
        let ents = crate::entities::write_entities(&[
            ent(&[("classname", "worldspawn")]),
            ent(&[("classname", "logic_relay"), ("targetname", "r"), ("OnTrigger", "door,Open,,0,-1")]),
            ent(&[("classname", "func_door"), ("targetname", "door"), ("model", "*1"), ("parentname", "train")]),
        ]);
        let mut data = Vec::new();
        let mut table = Vec::new();
        for n in ["custom/brick", "brick/wall"] {
            table.extend_from_slice(&(data.len() as u32).to_le_bytes());
            data.extend_from_slice(n.as_bytes());
            data.push(0);
        }
        let mut pak = crate::PakWriter::new();
        pak.add("materials/custom/brick.vmt", b"\"LightmappedGeneric\"\n{\n}\n".to_vec()).unwrap();
        let zip = pak.finish(crate::PakOptions::default()).unwrap();
        let sprp = crate::GameLump::new(u32::from_be_bytes(*b"sprp"), 10, vec![9u8; 40]).unwrap();
        let game = crate::GameLumps { lumps: vec![sprp] }.to_bytes(0).unwrap();
        let full = Bsp::from_vec(crate::synthetic_bsp(
            20,
            &[
                (lump::ENTITIES, ents),
                (lump::ORIGINAL_FACES, vec![1u8; 64]),
                (lump::GAME_LUMP, game),
                (lump::TEXDATA_STRING_DATA, data),
                (lump::TEXDATA_STRING_TABLE, table),
                (lump::PAKFILE, zip),
            ],
        ))
        .unwrap();
        let opts = ProtectOptions {
            watermark: "my server".into(),
            strip_original_faces: true,
            randomize_materials: true,
            salt: "s".into(),
            lock: Some(Lock { hash: lock_hash("key"), block: false }),
            ..Default::default()
        };
        let (client_bytes, _) = protect(&full, &opts).unwrap();
        let server_bytes = server_copy(&client_bytes, &full, &opts).unwrap();
        let client = Bsp::from_vec(client_bytes).unwrap();
        let server = Bsp::from_vec(server_bytes).unwrap();
        for i in (0..crate::LUMP_COUNT).filter(|&i| i != lump::ENTITIES) {
            assert_eq!(client.lump_raw(i).unwrap(), server.lump_raw(i).unwrap(), "lump {i} differs");
            assert_eq!(client.header().lumps[i].offset, server.header().lumps[i].offset, "lump {i} moved");
        }
        assert_eq!(server.game_lumps().unwrap().lumps.len(), 1, "the game lump still parses");
        let find = |b: &Bsp, class: &str| b.entities().unwrap().into_iter().find(|e| e.classname() == class);
        assert!(find(&client, "logic_relay").is_none(), "the client copy has no logic");
        let relay = find(&server, "logic_relay").expect("the server keeps the logic");
        assert_eq!(relay.get("OnTrigger"), Some("door,Open,,0,-1"), "and its outputs");
        assert_eq!(find(&server, "func_door").unwrap().get("parentname"), Some("train"), "and parenting");
        for b in [&client, &server] {
            let world = find(b, "worldspawn").unwrap();
            assert_eq!(world.get("hammerpro_protected"), Some("my server"));
            assert!(world.get("hammerpro_lock").is_some());
        }
    }
}
