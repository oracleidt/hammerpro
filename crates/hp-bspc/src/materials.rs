pub mod contents {
    pub const EMPTY: u32 = 0;
    pub const SOLID: u32 = 0x1;
    pub const WINDOW: u32 = 0x2;
    pub const GRATE: u32 = 0x8;
    pub const SLIME: u32 = 0x10;
    pub const WATER: u32 = 0x20;
    pub const BLOCKLOS: u32 = 0x40;
    pub const OPAQUE: u32 = 0x80;
    pub const MOVEABLE: u32 = 0x4000;
    pub const AREAPORTAL: u32 = 0x8000;
    pub const PLAYERCLIP: u32 = 0x10000;
    pub const MONSTERCLIP: u32 = 0x20000;
    pub const ORIGIN: u32 = 0x0100_0000;
    pub const DETAIL: u32 = hp_vmf::CONTENTS_DETAIL;
    pub const TRANSLUCENT: u32 = 0x1000_0000;
    pub const LADDER: u32 = 0x2000_0000;
    pub const TESTFOGVOLUME: u32 = 0x100;
    pub const MASK_WATER: u32 = WATER | SLIME;
    pub const MASK_SOLID: u32 = SOLID | MOVEABLE | WINDOW | GRATE | 0x0200_0000;
    pub const STRUCTURAL: u32 = SOLID | WINDOW | GRATE | WATER | SLIME;
}

pub mod surf {
    pub const LIGHT: u32 = 0x1;
    pub const SKY2D: u32 = 0x2;
    pub const SKY: u32 = 0x4;
    pub const WARP: u32 = 0x8;
    pub const TRANS: u32 = 0x10;
    pub const NOPORTAL: u32 = 0x20;
    pub const TRIGGER: u32 = 0x40;
    pub const NODRAW: u32 = 0x80;
    pub const HINT: u32 = 0x100;
    pub const SKIP: u32 = 0x200;
    pub const NOLIGHT: u32 = 0x400;
    pub const BUMPLIGHT: u32 = 0x800;
    pub const NOSHADOWS: u32 = 0x1000;
    pub const NODECALS: u32 = 0x2000;
    pub const NOCHOP: u32 = 0x4000;
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialFacts {
    pub contents: u32,
    pub raw_contents: u32,
    pub surf: u32,
    pub width: i32,
    pub height: i32,
    pub reflectivity: [f32; 3],
    pub surfaceprop: String,
    pub has_surfaceprop: bool,
    pub surfaceprop2: Option<String>,
    pub found: bool,
    pub envmap: Option<String>,
    pub detail_type: Option<String>,
    pub macro_texture: Option<String>,
    pub bottom_material: Option<String>,
    pub shader: String,
}

impl Default for MaterialFacts {
    fn default() -> Self {
        Self {
            contents: contents::SOLID,
            raw_contents: 0,
            surf: 0,
            width: 64,
            height: 64,
            reflectivity: [0.18; 3],
            surfaceprop: String::new(),
            has_surfaceprop: false,
            surfaceprop2: None,
            found: false,
            envmap: None,
            detail_type: None,
            macro_texture: None,
            bottom_material: None,
            shader: String::new(),
        }
    }
}

pub trait MaterialSource: Sync {
    fn facts(&self, name: &str) -> MaterialFacts;

    fn read_file(&self, _path: &str) -> Option<Vec<u8>> {
        None
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MaterialOptions {
    pub bump_all: bool,
    pub light_if_missing: bool,
    pub nodraw_triggers: bool,
}

pub fn facts_from_vmt(
    shader: &str,
    key: &dyn Fn(&str) -> Option<String>,
    size: Option<(i32, i32)>,
    reflectivity: Option<[f32; 3]>,
) -> MaterialFacts {
    facts_from_vmt_with(shader, key, size, reflectivity, &MaterialOptions::default())
}

pub fn facts_from_vmt_with(
    shader: &str,
    key: &dyn Fn(&str) -> Option<String>,
    size: Option<(i32, i32)>,
    reflectivity: Option<[f32; 3]>,
    opts: &MaterialOptions,
) -> MaterialFacts {
    let flag = |k: &str| key(k).is_some_and(|v| v.trim() != "0" && !v.trim().is_empty());
    let mut f = MaterialFacts { found: true, ..MaterialFacts::default() };
    if let Some((w, h)) = size {
        f.width = w.max(1);
        f.height = h.max(1);
    }
    if let Some(r) = reflectivity {
        f.reflectivity = r;
    }
    if let Some(sp) = key("$surfaceprop") {
        f.surfaceprop = sp.trim().to_owned();
        f.has_surfaceprop = true;
    }
    f.surfaceprop2 = key("$surfaceprop2").map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    f.detail_type = key("%detailtype").map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
    f.macro_texture = key("$macro_texture").map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
    f.envmap = key("$envmap").map(|e| e.trim().to_owned());
    f.bottom_material = key("$bottommaterial").map(|b| b.trim().to_owned()).filter(|b| !b.is_empty());
    let shader = shader.to_ascii_lowercase();
    f.shader = shader.clone();
    let mut c = 0u32;
    let mut s = 0u32;
    if flag("%compilesky") {
        s |= surf::SKY | surf::NOLIGHT;
    } else if flag("%compile2dsky") {
        s |= surf::SKY | surf::SKY2D | surf::NOLIGHT;
    } else if flag("%compilehint") {
        s |= surf::NODRAW | surf::NOLIGHT | surf::HINT;
    } else if flag("%compileskip") {
        s |= surf::NODRAW | surf::NOLIGHT | surf::SKIP;
    } else if flag("%compileorigin") {
        c |= contents::ORIGIN | contents::DETAIL;
        s |= surf::NODRAW | surf::NOLIGHT;
    } else if flag("%compileclip") {
        c |= contents::PLAYERCLIP | contents::MONSTERCLIP;
        s |= surf::NODRAW | surf::NOLIGHT;
    } else if flag("%playerclip") {
        c |= contents::PLAYERCLIP;
        s |= surf::NODRAW | surf::NOLIGHT;
    } else if flag("%compilenpcclip") {
        c |= contents::MONSTERCLIP;
        s |= surf::NODRAW | surf::NOLIGHT;
    } else if flag("%compilenochop") {
        s |= surf::NOCHOP;
    } else if flag("%compiletrigger") {
        s |= surf::NOLIGHT | surf::TRIGGER;
        if opts.nodraw_triggers {
            s |= surf::NODRAW;
        }
    } else if flag("%compilenolight") && !flag("%compilewater") {
        s |= surf::NOLIGHT;
    } else {
        if flag("%compileladder") {
            c |= contents::LADDER;
        }
        if flag("%noportal") {
            s |= surf::NOPORTAL;
        }
        if flag("%compilepassbullets") {
            c = (c & !contents::SOLID) | contents::GRATE;
        }
        let lightmapped = [
            "lightmappedgeneric",
            "worldvertextransition",
            "lightmapped_4wayblend",
            "lightmappedtwotexture",
            "worldtwotextureblend",
        ]
        .iter()
        .any(|n| shader.contains(n))
            || (shader.starts_with("water") && flag("$lightmapwaterfog"));
        if opts.bump_all
            || (lightmapped
                && !shader.starts_with("water")
                && (key("$bumpmap").is_some() || key("$normalmap").is_some()))
        {
            s |= surf::BUMPLIGHT;
        }
        if lightmapped {
            s &= !surf::NOLIGHT;
        } else if !opts.light_if_missing {
            s |= surf::NOLIGHT;
        }
        if flag("%compilenodraw") {
            s |= surf::NODRAW | surf::NOLIGHT;
        }
        if flag("%compileinvisible") {
            c = (c & !contents::SOLID) | contents::GRATE;
            s |= surf::NODRAW | surf::NOLIGHT;
        }
        let mut check_window = true;
        if flag("%compilenonsolid") {
            c = contents::OPAQUE;
            check_window = false;
        }
        if flag("%compileblocklos") {
            c = contents::BLOCKLOS;
            check_window = false;
        }
        if flag("%compiledetail") {
            c |= contents::DETAIL;
        }
        let keep_light = flag("%compilekeeplight");
        if flag("%compilewater") {
            c = (c & !(contents::SOLID | contents::DETAIL)) | contents::WATER;
            s |= surf::WARP | surf::NOSHADOWS | surf::NODECALS;
        }
        let waterfog = flag("$lightmapwaterfog");
        if (!keep_light && !waterfog && shader.starts_with("water")) || shader.starts_with("unlitgeneric") {
            s |= surf::NOLIGHT;
        }
        if flag("%compileslime") {
            c = (c & !(contents::SOLID | contents::DETAIL)) | contents::SLIME;
            s |= surf::NODECALS;
        }
        let translucent =
            flag("$translucent") || key("$alpha").and_then(|a| a.trim().parse::<f32>().ok()).is_some_and(|a| a < 1.0);
        let alpha_tested = flag("$alphatest");
        if check_window && (translucent || alpha_tested) {
            if c & (contents::GRATE | contents::WATER) == 0 {
                c |= contents::WINDOW;
            }
            c &= !contents::SOLID;
            if translucent {
                s |= surf::TRANS;
            }
        }
        if s & surf::NOLIGHT != 0 {
            s &= !surf::BUMPLIGHT;
        }
    }
    if flag("%compilenoshadows") {
        s |= surf::NOSHADOWS;
    }
    f.raw_contents = c;
    f.contents = side_contents(c, s);
    f.surf = s;
    f
}

pub fn side_contents(raw: u32, surf_flags: u32) -> u32 {
    let mut c = raw;
    if c & (contents::PLAYERCLIP | contents::MONSTERCLIP) != 0 {
        c |= contents::DETAIL;
    }
    if c & (0xff | contents::PLAYERCLIP | contents::MONSTERCLIP) == 0 {
        c |= contents::SOLID;
    }
    if surf_flags & (surf::HINT | surf::SKIP) != 0 {
        c = contents::EMPTY;
    }
    c
}

pub fn material_path(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let c = if c == '\\' { '/' } else { c };
        if c == '/' && (out.is_empty() || out.ends_with('/')) {
            continue;
        }
        out.push(c);
    }
    out
}

pub fn facts_by_name(name: &str) -> MaterialFacts {
    let n = name.to_ascii_lowercase().replace('\\', "/");
    let n = n.trim_start_matches("materials/");
    let tool = |s: &str| n == format!("tools/{s}");
    let mut f = MaterialFacts::default();
    let (c, s) = if tool("toolsnodraw") {
        (contents::SOLID, surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsclip") {
        (contents::PLAYERCLIP | contents::MONSTERCLIP, surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsplayerclip") {
        (contents::PLAYERCLIP, surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsnpcclip") {
        (contents::MONSTERCLIP, surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsinvisible") {
        (contents::SOLID, surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsinvisibleladder") {
        (contents::LADDER | contents::GRATE | contents::TRANSLUCENT, surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsskybox") {
        (contents::SOLID, surf::SKY | surf::NOLIGHT)
    } else if tool("toolsskybox2d") {
        (contents::SOLID, surf::SKY | surf::SKY2D | surf::NOLIGHT)
    } else if tool("toolshint") {
        (contents::EMPTY, surf::HINT | surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsskip") {
        (contents::EMPTY, surf::SKIP | surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolstrigger") || tool("toolsfog") {
        (contents::SOLID, surf::TRIGGER | surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsareaportal") {
        (contents::AREAPORTAL, surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsorigin") {
        (contents::ORIGIN, surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsblocklight") {
        (contents::OPAQUE, surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsblock_los") || tool("toolsblocklos") {
        (contents::BLOCKLOS, surf::NODRAW | surf::NOLIGHT)
    } else if tool("toolsoccluder") {
        (contents::SOLID | contents::TRANSLUCENT, surf::TRANS | surf::NOLIGHT)
    } else {
        (contents::SOLID, 0)
    };
    f.raw_contents = c & !contents::SOLID;
    f.contents = side_contents(f.raw_contents, s);
    f.surf = s;
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compile_keys_and_tool_names() {
        let vmt = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(a, _)| *a == k).map(|(_, v)| (*v).to_owned())
        };
        let clip = facts_from_vmt("LightmappedGeneric", &vmt(&[("%compileclip", "1")]), None, None);
        assert_eq!(clip.contents, contents::PLAYERCLIP | contents::MONSTERCLIP | contents::DETAIL);
        assert!(clip.surf & surf::NODRAW != 0);
        let water = facts_from_vmt("Water", &vmt(&[("%compilewater", "1")]), None, None);
        assert_eq!(water.contents, contents::WATER);
        let unlit = facts_from_vmt("Water", &vmt(&[]), None, None);
        assert_eq!((unlit.contents, unlit.surf), (contents::SOLID, surf::NOLIGHT));
        let bump = facts_from_vmt("LightmappedGeneric", &vmt(&[("$bumpmap", "x")]), Some((512, 256)), None);
        assert!(bump.surf & surf::BUMPLIGHT != 0);
        assert_eq!((bump.width, bump.height), (512, 256));
        let glass = facts_from_vmt("LightmappedGeneric", &vmt(&[("$translucent", "1")]), None, None);
        assert!(glass.contents == contents::WINDOW && glass.surf & surf::TRANS != 0);
        assert_eq!(
            crate::brush::MapBrush::contents_from_sides(&[glass.contents]).0,
            contents::WINDOW | contents::TRANSLUCENT
        );
        let ap =
            facts_from_vmt("LightmappedGeneric", &vmt(&[("$translucent", "1"), ("%compilenolight", "1")]), None, None);
        assert_eq!((ap.contents, ap.surf), (contents::SOLID, surf::NOLIGHT));
        assert_eq!(facts_by_name("TOOLS/toolsSkybox").surf & surf::SKY, surf::SKY);
        assert_eq!(facts_by_name("tools/toolshint").contents, contents::EMPTY);
        assert_eq!(facts_by_name("brick/wall01").contents, contents::SOLID);
        let grass = facts_from_vmt("WorldVertexTransition", &vmt(&[("%detailtype", "grassland1")]), None, None);
        assert_eq!(grass.detail_type.as_deref(), Some("grassland1"));
        assert_eq!(facts_by_name("brick/wall01").detail_type, None);
    }

    #[test]
    fn gmod_keys_and_material_options() {
        let vmt = |pairs: &'static [(&'static str, &'static str)]| {
            move |k: &str| pairs.iter().find(|(a, _)| *a == k).map(|(_, v)| (*v).to_owned())
        };
        let ns = facts_from_vmt("LightmappedGeneric", &vmt(&[("%compilenoshadows", "1")]), None, None);
        assert_eq!(ns.surf, surf::NOSHADOWS);
        assert_eq!(ns.contents, contents::SOLID);
        let fog = facts_from_vmt("Water", &vmt(&[("%compilewater", "1"), ("$lightmapwaterfog", "1")]), None, None);
        assert_eq!(fog.surf & surf::NOLIGHT, 0);
        let plain = facts_from_vmt("Water", &vmt(&[("%compilewater", "1")]), None, None);
        assert_ne!(plain.surf & surf::NOLIGHT, 0);
        let all = MaterialOptions { bump_all: true, light_if_missing: true, nodraw_triggers: true };
        let flat = facts_from_vmt_with("LightmappedGeneric", &vmt(&[]), None, None, &all);
        assert_ne!(flat.surf & surf::BUMPLIGHT, 0);
        let unlit = facts_from_vmt_with(
            "VertexLitGeneric",
            &vmt(&[]),
            None,
            None,
            &MaterialOptions { light_if_missing: true, ..Default::default() },
        );
        assert_eq!(unlit.surf & surf::NOLIGHT, 0);
        assert_ne!(facts_from_vmt("VertexLitGeneric", &vmt(&[]), None, None).surf & surf::NOLIGHT, 0);
        let trigger = facts_from_vmt_with("LightmappedGeneric", &vmt(&[("%compiletrigger", "1")]), None, None, &all);
        assert_eq!(trigger.surf, surf::NOLIGHT | surf::TRIGGER | surf::NODRAW);
        let trigger = facts_from_vmt("LightmappedGeneric", &vmt(&[("%compiletrigger", "1")]), None, None);
        assert_eq!(trigger.surf, surf::NOLIGHT | surf::TRIGGER);
    }
}
