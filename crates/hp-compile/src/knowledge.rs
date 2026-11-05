#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    GoToLocation,
    OpenProblems,
    OpenDocs(&'static str),
    LoadLeak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Knowledge {
    pub code: &'static str,
    pub english: &'static str,
    pub fix: Fix,
}

const fn kb(code: &'static str, english: &'static str, fix: Fix) -> Knowledge {
    Knowledge { code, english, fix }
}

const VDC_LIMITS: &str = "https://developer.valvesoftware.com/wiki/Source_Engine_limits";
const VDC_VBSP: &str = "https://developer.valvesoftware.com/wiki/VBSP";
const VDC_VIS: &str = "https://developer.valvesoftware.com/wiki/Visibility_optimization";

const RULES: &[(&str, Knowledge)] = &[
    (
        "**** leaked ****",
        kb(
            "leak",
            "An entity can reach the void through a hole in the world brushes: the visibility cannot be computed and the map renders and lights badly. Follow the leak path from the entity to the hole and seal it with a world brush (func_detail and brush entities do not seal).",
            Fix::LoadLeak,
        ),
    ),
    (
        "leaked!",
        kb(
            "leak_entity",
            "This entity is the start of the leak path: the hole is somewhere between it and the void. Follow the leak path and seal the hole with a world brush.",
            Fix::LoadLeak,
        ),
    ),
    (
        "areaportal leak",
        kb(
            "areaportal_leak",
            "An area portal does not close its opening completely: the two areas touch elsewhere. Make sure every areaportal fills its opening and the areas are separated by world brushes.",
            Fix::LoadLeak,
        ),
    ),
    (
        "microbrush",
        kb(
            "microbrush",
            "A brush is too small or too thin to be built: delete it or make it at least 1 unit thick on the grid.",
            Fix::GoToLocation,
        ),
    ),
    (
        "texture axis perpendicular",
        kb(
            "texture_axis",
            "A face's texture is projected along the face itself, so it would be stretched infinitely. Reset the face alignment (the problem checker fixes it).",
            Fix::OpenProblems,
        ),
    ),
    (
        "mixed face contents",
        kb(
            "mixed_contents",
            "A brush has faces with materials of different contents (water and solid, nodraw and clip...). Use one kind of material on every face of the brush.",
            Fix::GoToLocation,
        ),
    ),
    (
        "displacement found on a(n)",
        kb(
            "disp_detail",
            "Displacements cannot be part of a brush entity: move the displacement brush back to the world.",
            Fix::GoToLocation,
        ),
    ),
    (
        "too many switched lights",
        kb(
            "switched_lights",
            "More than 96 named lights (lights with a name, which can be turned on and off): give lights that switch together the same name, or remove the name of lights that never switch.",
            Fix::OpenProblems,
        ),
    ),
    (
        "must be used on a dynamic entity",
        kb(
            "dynamic_prop",
            "This model has physics properties (prop_data) and cannot be a prop_static: the props are deleted. Use prop_physics or prop_dynamic, or compile with -AllowDynamicPropsAsStatic.",
            Fix::OpenProblems,
        ),
    ),
    (
        "must be compiled with $staticprop",
        kb(
            "not_static_prop",
            "This model was not compiled as a static prop and cannot be a prop_static: the props are deleted. Use prop_dynamic.",
            Fix::OpenProblems,
        ),
    ),
    (
        "overlay touching too many faces",
        kb(
            "overlay_faces",
            "An overlay covers 64 faces or more: make it smaller, or remove faces from its list.",
            Fix::GoToLocation,
        ),
    ),
    (
        "too many overlays",
        kb(
            "overlays_limit",
            "The map has more than 8192 overlays: remove some, or merge decals into fewer overlays.",
            Fix::OpenDocs(VDC_LIMITS),
        ),
    ),
    (
        "too many brush models",
        kb(
            "brush_models",
            "The map has more than 4096 brush models (brush entities, the world included): turn brush entities that do not move into world brushes or func_detail.",
            Fix::OpenDocs(VDC_LIMITS),
        ),
    ),
    (
        "could not open instance file",
        kb(
            "instance_file",
            "The file of a func_instance cannot be found: check its path (relative to the map, or to the maps folder) and that the file exists.",
            Fix::OpenProblems,
        ),
    ),
    (
        "bad surface extents",
        kb(
            "surface_extents",
            "A face is too big for its lightmap (more than 126 luxels on a side) and cannot be cut (-nosubdiv): raise its lightmap scale or split the brush.",
            Fix::GoToLocation,
        ),
    ),
    (
        "unknown option",
        kb(
            "unknown_option",
            "The compiler does not know this option: it was ignored. Check the spelling in the compile settings.",
            Fix::OpenDocs(VDC_VBSP),
        ),
    ),
    (
        "material not found",
        kb(
            "missing_material",
            "A material does not exist in the game or in your custom folders: the faces using it compile without its properties. Replace it, or add its folder in the settings.",
            Fix::OpenProblems,
        ),
    ),
    (
        "error loading studio model",
        kb(
            "missing_model",
            "A model used by a prop does not exist or is broken: the prop is left out of the map.",
            Fix::OpenProblems,
        ),
    ),
    (
        "outside the map",
        kb(
            "prop_outside",
            "A static prop is in the void: it is removed from the map. Move it inside the world.",
            Fix::GoToLocation,
        ),
    ),
    (
        "in solid volume",
        kb(
            "entity_in_solid",
            "An entity is inside a solid brush: it cannot see the map (lighting and sky camera break). Move it into open space.",
            Fix::GoToLocation,
        ),
    ),
    (
        "findportalside",
        kb(
            "portal_side",
            "The compiler could not tell which brush a portal belongs to, often because of overlapping or invalid brushes near this point. Clean up the brushes there.",
            Fix::GoToLocation,
        ),
    ),
    (
        "brush with win 0",
        kb(
            "bad_brush",
            "A brush has a face of zero area: it is invalid. Delete it or rebuild it with the block tool.",
            Fix::GoToLocation,
        ),
    ),
    (
        "skybox fallback:",
        kb(
            "skybox_fallback",
            "The requested sky has missing or invalid materials or textures. The compiled map uses an available fallback sky; the source VMF keeps its skyname. Mount the original sky's content to restore its appearance.",
            Fix::OpenProblems,
        ),
    ),
    (
        "lightmaps are entirely black",
        kb(
            "black_lightmaps",
            "This lighting pass contains no lit surfaces. Check light entities, light_environment, openings to tools/toolsskybox and HDR brightness scales. A missing default reflection cubemap is a separate issue.",
            Fix::OpenProblems,
        ),
    ),
    (
        "to build the default cubemap",
        kb(
            "skybox_cubemap",
            "The default reflection cubemap could not be built from the sky materials. Check the mounted sky textures and buildcubemaps in game. This does not disable baked surface lighting; an entirely black map needs a separate lighting check.",
            Fix::OpenDocs("https://developer.valvesoftware.com/wiki/Cubemaps"),
        ),
    ),
    (
        "skybox vtf files for",
        kb(
            "skybox_cubemap",
            "Harmless with the Garry's Mod skies: the game builds the cubemaps itself (buildcubemaps).",
            Fix::OpenDocs("https://developer.valvesoftware.com/wiki/Cubemaps"),
        ),
    ),
    (
        "max_map_brushes",
        kb(
            "limit_brushes",
            "Too many brushes for the engine. Merge brushes, turn decorative ones into props, or delete hidden ones.",
            Fix::OpenDocs(VDC_LIMITS),
        ),
    ),
    (
        "max_map_planes",
        kb(
            "limit_planes",
            "Too many unique planes. Keep brushes on the grid and simplify off-grid or rotated geometry.",
            Fix::OpenDocs(VDC_LIMITS),
        ),
    ),
    (
        "max_map_portals",
        kb(
            "limit_portals",
            "Too many visibility portals. Turn small or detailed brushes into func_detail.",
            Fix::OpenDocs(VDC_VIS),
        ),
    ),
    (
        "max_map_portalverts",
        kb(
            "limit_portals",
            "Too many visibility portals. Turn small or detailed brushes into func_detail.",
            Fix::OpenDocs(VDC_VIS),
        ),
    ),
    (
        "max_map_lighting",
        kb(
            "limit_lighting",
            "The lightmaps are too large. Raise the lightmap scale of large or unseen faces (32 or more), or use nodraw where players never look.",
            Fix::OpenDocs(VDC_LIMITS),
        ),
    ),
    (
        "max_map_",
        kb(
            "limit",
            "A hard engine limit was exceeded: the map cannot be compiled until something is reduced.",
            Fix::OpenDocs(VDC_LIMITS),
        ),
    ),
    (
        "too many brushes in one leaf",
        kb(
            "leaf_brushes",
            "Too many brushes touch one visibility leaf. Turn small brushes in this area into func_detail.",
            Fix::GoToLocation,
        ),
    ),
    (
        "out of memory",
        kb(
            "out_of_memory",
            "The compiler ran out of memory. Close other programs, lower the number of threads, or split very large maps.",
            Fix::OpenDocs(VDC_LIMITS),
        ),
    ),
];

pub fn explain(line: &str) -> Option<&'static Knowledge> {
    let lower = line.to_ascii_lowercase();
    RULES.iter().find(|(pattern, _)| lower.contains(pattern)).map(|(_, k)| k)
}

pub fn codes() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = RULES.iter().map(|(_, k)| k.code).collect();
    v.sort_unstable();
    v.dedup();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_are_explained() {
        assert_eq!(explain("**** leaked ****").map(|k| k.code), Some("leak"));
        assert_eq!(explain("Entity info_player_start (-680.00 0.00 8.00) leaked!").map(|k| k.fix), Some(Fix::LoadLeak));
        assert_eq!(explain("Brush 12: WARNING, microbrush").map(|k| k.code), Some("microbrush"));
        assert_eq!(explain("ERROR: MAX_MAP_PLANES").map(|k| k.code), Some("limit_planes"));
        assert_eq!(explain("MAX_MAP_TEXINFO exceeded").map(|k| k.code), Some("limit"));
        assert_eq!(explain("WARNING: material not found: \"dev/x\"").map(|k| k.fix), Some(Fix::OpenProblems));
        assert!(explain("Building Faces...done (0)").is_none());
        assert_eq!(
            explain("Too many switched lights (error at light sw96), max = 96").map(|k| k.code),
            Some("switched_lights")
        );
        assert_eq!(
            explain("Error: displacement found on a(n) func_brush entity - not supported (entity 2, brush 0)")
                .map(|k| k.code),
            Some("disp_detail")
        );
        assert_eq!(
            explain("Error! prop_static using model \"models/x.mdl\", which must be used on a dynamic entity (i.e. prop_physics). Deleted.").map(|k| k.code),
            Some("dynamic_prop")
        );
        assert_eq!(
            explain("Overlay touching too many faces (touching 70, max 64)").map(|k| k.code),
            Some("overlay_faces")
        );
        assert_eq!(explain("Too Many Overlays!").map(|k| k.code), Some("overlays_limit"));
        assert_eq!(explain("Too many brush models in map, max = 4096").map(|k| k.code), Some("brush_models"));
        assert_eq!(explain("Could not open instance file instances/a.vmf").map(|k| k.code), Some("instance_file"));
        assert_eq!(
            explain("Bad surface extents - surface is too big to have a lightmap").map(|k| k.code),
            Some("surface_extents")
        );
        assert_eq!(explain("Warning: VBSP: Unknown option \"-foo\" (ignored)").map(|k| k.code), Some("unknown_option"));
        assert!(codes().len() >= 20);
        for (_, k) in RULES {
            assert!(!k.english.is_empty() && !k.english.contains('\u{2014}'));
        }
    }
}
