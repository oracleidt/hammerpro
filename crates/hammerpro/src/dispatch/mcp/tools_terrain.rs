use hp_app::AppState;
use hp_geom::{AutoTexture, FalloffShape, NoiseBasis, NoiseShape, TerrainPreset};
use hp_mcp::protocol::ToolResult;
use hp_tools_mesh::terrain::export::{ExportOptions, export};
use hp_tools_mesh::terrain::generate::{GenOptions, generate_as, preview};
use hp_tools_mesh::terrain::import::{ImportOptions, height_field, import_as};
use hp_tools_mesh::terrain::{TexMode, TileSetup};
use serde_json::json;

use super::semantic::*;
use super::tools::{Args, R, undo_label};

const MAX_PATH: usize = 1024;

domain! { "terrain";
    import_heightmap: Edit, "Import heightmap",
        "Build a terrain from a heightmap image, like Tools > Import heightmap: a grid of sewn displacement tiles in one undo step. path is a PNG (8 or 16 bit gray, or color read as luminance), TIFF, EXR, HDR or 16 bit RAW file (RAW: raw_width and raw_height, both 0 for a square image, raw_big_endian for the byte order), full or relative to the map folder. The image maps to size_x by size_y units (default 4096 by 4096), centered on origin (centered, default true) or with origin as the lowest X and Y corner. Black becomes min_height and white max_height (default 0 and 512; height_scale sets max_height as min_height plus the scale); origin z is where height 0 sits. The terrain is cut into tiles of about tile_size units (default 512) of displacement power tile_power (2, 3 or 4, default 3), so the vertex spacing is tile_size divided by 2 to the power tile_power; image samples between pixels use bicubic sampling (sampling bilinear to change). invert, flip_x, flip_y and smoothing (blur passes) shape the heights; the image top is the north (+Y) edge. skirt lowers the outer ring of vertices, thickness is the brush depth under the tiles, material and lightmap_scale are those of the faces. texturing none (default), auto (rock on steep slopes, grass on flat ground, snow above snow_height, sand below sand_height; thresholds rock_slope and rock_blend in degrees, snow_height, snow_blend, sand_height, sand_blend as shares of the height range) or splat (splat is an RGB or RGBA image whose red, green, blue and alpha drive the 4-way blend channels 1 to 4, or splat_channels is four grayscale images, empty strings for unused ones). Texturing writes the 4-way blend rows and the alpha. preview true only reports the tile layout (tiles, displacements, vertex spacing, image size) and creates nothing. The new tiles are selected.",
        {
            "path": string("Heightmap file: full path, or relative to the map folder."),
            "raw_width": integer("RAW only: pixels per row (0 with raw_height 0: square).", 0, 16384),
            "raw_height": integer("RAW only: pixel rows.", 0, 16384),
            "raw_big_endian": boolean("RAW only: most significant byte first (default false)."),
            "size_x": number("Terrain size along X in units (default 4096).", 16.0, 32768.0),
            "size_y": number("Terrain size along Y in units (default 4096).", 16.0, 32768.0),
            "origin": vec3("Position of the terrain (default [0, 0, 0])."),
            "centered": boolean("origin is the center (default true) instead of the lowest X and Y corner."),
            "min_height": number("Height of black (default 0).", -16384.0, 16384.0),
            "max_height": number("Height of white (default 512).", -16384.0, 16384.0),
            "height_scale": number("Alternative to max_height: max_height = min_height + height_scale.", 0.0, 32768.0),
            "tile_power": integer("Displacement power of the tiles (default 3).", 2, 4),
            "tile_size": number("Wanted tile size in units (default 512).", 16.0, 16384.0),
            "invert": boolean("White becomes low (default false)."),
            "flip_x": boolean("Mirror along X."), "flip_y": boolean("Mirror along Y."),
            "smoothing": integer("Blur passes on the heights (default 0).", 0, 64),
            "sampling": choice("Sampling between pixels (default bicubic).", &["bicubic", "bilinear"]),
            "skirt": number("Units the outermost vertex ring is lowered (default 0).", 0.0, 4096.0),
            "thickness": number("Brush depth under the tiles (default 16).", 1.0, 4096.0),
            "material": string("Material of the displacement faces (default DEV/DEV_BLENDMEASURE)."),
            "lightmap_scale": integer("Lightmap scale of the faces (default 16).", 1, 1024),
            "texturing": choice("Texturing (default none).", &["none", "auto", "splat"]),
            "rock_slope": number("auto: slope in degrees where rock starts (default 35).", 0.0, 89.0),
            "rock_blend": number("auto: rock transition width in degrees (default 10).", 0.0, 45.0),
            "snow_height": number("auto: share of the height range above which snow appears (default 0.8).", 0.0, 1.0),
            "snow_blend": number("auto: snow transition width (default 0.1).", 0.0, 1.0),
            "sand_height": number("auto: share of the height range below which sand appears, 0 is off (default 0).", 0.0, 1.0),
            "sand_blend": number("auto: sand transition width (default 0.05).", 0.0, 1.0),
            "splat": string("splat: RGB or RGBA image, full path or relative to the map folder."),
            "splat_channels": strings("splat: four grayscale images for blend channels 1 to 4 (empty string: none).", 4),
            "preview": boolean("Only report the tile layout, create nothing (default false)."),
        }, ["path"];
    export_heightmap: Editor, "Export heightmap",
        "Write the heights of displacements to a 16 bit heightmap, the reverse of import_heightmap (Tools > Export heightmap). Targets: faces, ids or solid_ids, else the selected displacements or those of the selected solids. path is a full path or relative to the map folder; the extension picks the format: png (16 bit gray), tif or tiff (16 bit), raw (16 bit little endian). The image top is the north (+Y) edge and its pixels follow the finest vertex spacing, so the displacements of an imported terrain come back one pixel per vertex. Black is min_height and white is max_height (default: the lowest and highest point); the result reports them with the size and the lowest X and Y corner, which are the import_heightmap values (min_height, max_height, size_x, size_y, origin with centered false) that rebuild the same terrain. splat_path also writes the 4-way blend weights (or the alpha) as an RGBA PNG: red, green, blue and alpha are blend channels 1 to 4. overwrite replaces existing files (default false).",
        {
            "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(),
            "path": string("Heightmap file: full path, or relative to the map folder."),
            "splat_path": string("Optional RGBA PNG of the blend weights."),
            "min_height": number("Height written as black (default: the lowest point).", -16384.0, 16384.0),
            "max_height": number("Height written as white (default: the highest point).", -16384.0, 16384.0),
            "overwrite": boolean("Replace existing files (default false)."),
        }, ["path"];
    generate_terrain: Edit, "Generate terrain",
        "Generate a procedural terrain, like Tools > Generate terrain: fractal noise, optional erosion and automatic texturing, as a grid of sewn displacement tiles in one undo step, or by replacing the heights of the selected displacements (replace_selected, with faces, ids or solid_ids to choose them; the lowest point of the height range then lies on each face plane). preset loads the noise settings of rolling_hills (default), mountains, canyon, islands, dunes or plateau; the other noise settings override it: basis perlin or simplex, shape plain, ridged or billow, features (biggest features across the longest side), octaves, persistence, warp (domain warping), stretch (along Y), contrast, curve (height exponent), invert_noise, sea_level, terraces and terrace_sharpness, island with falloff_shape round or square, falloff and falloff_power (edge mask to zero). seed (0 to 4294967295, default random) makes the result repeatable: the same seed and settings give the same terrain, and the seed used is returned. The size, origin, centered, min_height, max_height, height_scale, tile_power, tile_size, skirt, thickness, material and lightmap_scale settings are those of import_heightmap (default height range 0 to 768). thermal_erosion and hydraulic_erosion switch the erosion passes on, with thermal_iterations, thermal_talus, thermal_strength and hydraulic_iterations, rain, evaporation, capacity, deposition, hydraulic_strength (same meaning as in the displacement tool options). auto_texture (default true) writes the 4-way blend rows from slope and height with the rock_slope, rock_blend, snow_height, snow_blend, sand_height and sand_blend thresholds of import_heightmap. preview show draws a low resolution preview of the terrain in the 3D view and creates nothing; preview hide removes it.",
        {
            "preset": choice("Noise preset (default rolling_hills).", &["rolling_hills", "mountains", "canyon", "islands", "dunes", "plateau"]),
            "seed": integer("Random seed (default: a new one each call).", 0, i64::from(u32::MAX)),
            "basis": choice("Noise basis.", &["perlin", "simplex"]),
            "shape": choice("Noise shape.", &["plain", "ridged", "billow"]),
            "features": number("Biggest features across the longest side.", 0.1, 64.0),
            "octaves": integer("Layers of detail.", 1, 10),
            "persistence": number("Weight of each finer layer.", 0.05, 1.0),
            "warp": number("Domain warping strength, 0 is off.", 0.0, 2.0),
            "stretch": number("Stretch along Y (above 1) or X (below 1).", 0.1, 16.0),
            "contrast": number("Spread of the noise over the height range.", 0.1, 4.0),
            "curve": number("Height exponent: above 1 flattens lowlands.", 0.2, 5.0),
            "invert_noise": boolean("Turn peaks into valleys."),
            "sea_level": number("Share of the range cut flat at the bottom.", 0.0, 0.95),
            "terraces": integer("Number of flat steps, 0 is off.", 0, 64),
            "terrace_sharpness": number("0 soft steps, 1 sharp cliffs.", 0.0, 1.0),
            "island": boolean("Fade the heights to zero toward the edges."),
            "falloff_shape": choice("Island mask shape.", &["round", "square"]),
            "falloff": number("Island mask width as a share of the radius.", 0.0, 1.0),
            "falloff_power": number("Island mask power.", 0.1, 8.0),
            "size_x": number("Terrain size along X in units (default 4096).", 16.0, 32768.0),
            "size_y": number("Terrain size along Y in units (default 4096).", 16.0, 32768.0),
            "origin": vec3("Position of the terrain (default [0, 0, 0])."),
            "centered": boolean("origin is the center (default true) instead of the lowest X and Y corner."),
            "min_height": number("Lowest height (default 0).", -16384.0, 16384.0),
            "max_height": number("Highest height (default 768).", -16384.0, 16384.0),
            "height_scale": number("Alternative to max_height: max_height = min_height + height_scale.", 0.0, 32768.0),
            "tile_power": integer("Displacement power of the tiles (default 3).", 2, 4),
            "tile_size": number("Wanted tile size in units (default 512).", 16.0, 16384.0),
            "skirt": number("Units the outermost vertex ring is lowered (default 0).", 0.0, 4096.0),
            "thickness": number("Brush depth under the tiles (default 16).", 1.0, 4096.0),
            "material": string("Material of the displacement faces (default DEV/DEV_BLENDMEASURE)."),
            "lightmap_scale": integer("Lightmap scale of the faces (default 16).", 1, 1024),
            "thermal_erosion": boolean("Run thermal erosion."),
            "thermal_iterations": integer("Thermal passes (default 20).", 1, 1000),
            "thermal_talus": number("Thermal talus angle in degrees (default 35).", 0.0, 89.0),
            "thermal_strength": number("Thermal strength (default 0.5).", 0.0, 1.0),
            "hydraulic_erosion": boolean("Run hydraulic erosion."),
            "hydraulic_iterations": integer("Hydraulic passes (default 50).", 1, 2000),
            "rain": number("Hydraulic rain (default 1).", 0.0, 100.0),
            "evaporation": number("Hydraulic evaporation (default 0.1).", 0.0, 1.0),
            "capacity": number("Hydraulic capacity (default 1).", 0.0, 10.0),
            "deposition": number("Hydraulic deposition (default 0.3).", 0.0, 1.0),
            "hydraulic_strength": number("Hydraulic strength (default 0.3).", 0.0, 1.0),
            "auto_texture": boolean("Write the 4-way blend from slope and height (default true)."),
            "rock_slope": number("Slope in degrees where rock starts (default 35).", 0.0, 89.0),
            "rock_blend": number("Rock transition width in degrees (default 10).", 0.0, 45.0),
            "snow_height": number("Share of the height range above which snow appears (default 0.8).", 0.0, 1.0),
            "snow_blend": number("Snow transition width (default 0.1).", 0.0, 1.0),
            "sand_height": number("Share of the height range below which sand appears, 0 is off (default 0).", 0.0, 1.0),
            "sand_blend": number("Sand transition width (default 0.05).", 0.0, 1.0),
            "replace_selected": boolean("Replace the heights of the selected displacements instead of creating tiles."),
            "faces": faces("With replace_selected: faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(),
            "preview": choice("Only show or hide the 3D preview, change nothing.", &["show", "hide"]),
        }, [];
}

fn safe_path(text: &str) -> R<()> {
    let p = std::path::Path::new(text.trim());
    if text.trim().is_empty() || p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(format!("invalid path \"{text}\""));
    }
    Ok(())
}

fn setup_from(a: &Args<'_>, s: &mut TileSetup) -> R<()> {
    if let Some(v) = a.opt_f64("size_x", 16.0, 32768.0)? {
        s.size.x = v;
    }
    if let Some(v) = a.opt_f64("size_y", 16.0, 32768.0)? {
        s.size.y = v;
    }
    if let Some(o) = a.opt_vec3("origin")? {
        s.origin = o;
    }
    if let Some(v) = a.opt_bool("centered")? {
        s.centered = v;
    }
    if let Some(v) = a.opt_f64("min_height", -16384.0, 16384.0)? {
        s.h_min = v;
        if a.get("max_height").is_none() && s.h_max < v {
            s.h_max = v;
        }
    }
    if let Some(v) = a.opt_f64("max_height", -16384.0, 16384.0)? {
        s.h_max = v;
    }
    if let Some(v) = a.opt_f64("height_scale", 0.0, 32768.0)? {
        if a.get("max_height").is_some() {
            return Err("give max_height or height_scale, not both".to_owned());
        }
        s.h_max = s.h_min + v;
    }
    if let Some(v) = a.opt_u64("tile_power", 2, 4)? {
        s.tile_power = v as u32;
    }
    if let Some(v) = a.opt_f64("tile_size", 16.0, 16384.0)? {
        s.tile_size = v;
    }
    if let Some(v) = a.opt_f64("skirt", 0.0, 4096.0)? {
        s.skirt = v;
    }
    if let Some(v) = a.opt_f64("thickness", 1.0, 4096.0)? {
        s.thickness = v;
    }
    if let Some(v) = a.opt_str("material", 128)? {
        s.material = v.to_owned();
    }
    if let Some(v) = a.opt_u64("lightmap_scale", 1, 1024)? {
        s.lightmap_scale = v as i32;
    }
    s.validate()
}

fn auto_from(a: &Args<'_>, t: &mut AutoTexture) -> R<()> {
    let fields: [(&str, f64, &mut f64); 6] = [
        ("rock_slope", 89.0, &mut t.rock_slope),
        ("rock_blend", 45.0, &mut t.rock_blend),
        ("snow_height", 1.0, &mut t.snow_height),
        ("snow_blend", 1.0, &mut t.snow_blend),
        ("sand_height", 1.0, &mut t.sand_height),
        ("sand_blend", 1.0, &mut t.sand_blend),
    ];
    for (key, max, slot) in fields {
        if let Some(v) = a.opt_f64(key, 0.0, max)? {
            *slot = v;
        }
    }
    Ok(())
}

fn import_options(a: &Args<'_>) -> R<ImportOptions> {
    let mut o = ImportOptions::default();
    let path = a.str("path", MAX_PATH)?;
    safe_path(path)?;
    o.path = path.to_owned();
    o.raw.width = a.opt_u64("raw_width", 0, 16384)?.unwrap_or(0) as u32;
    o.raw.height = a.opt_u64("raw_height", 0, 16384)?.unwrap_or(0) as u32;
    o.raw.big_endian = a.opt_bool("raw_big_endian")?.unwrap_or(false);
    setup_from(a, &mut o.setup)?;
    o.invert = a.opt_bool("invert")?.unwrap_or(false);
    o.flip_x = a.opt_bool("flip_x")?.unwrap_or(false);
    o.flip_y = a.opt_bool("flip_y")?.unwrap_or(false);
    o.smoothing = a.opt_u64("smoothing", 0, 64)?.unwrap_or(0) as u32;
    o.bilinear = a.opt_enum("sampling", &["bicubic", "bilinear"])?.as_deref() == Some("bilinear");
    if let Some(mode) = a.opt_enum("texturing", &["none", "auto", "splat"])? {
        o.tex.mode = TexMode::from_name(&mode).unwrap_or(TexMode::None);
    }
    auto_from(a, &mut o.tex.auto)?;
    if let Some(s) = a.opt_str("splat", MAX_PATH)? {
        safe_path(s)?;
        o.tex.splat = s.to_owned();
    }
    if let Some(list) = a.get("splat_channels") {
        let list =
            list.as_array().filter(|l| l.len() <= 4).ok_or("splat_channels must be a list of at most 4 paths")?;
        for (slot, v) in o.tex.channels.iter_mut().zip(list) {
            let s = v.as_str().ok_or("splat_channels must hold strings")?;
            if !s.trim().is_empty() {
                safe_path(s)?;
            }
            *slot = s.to_owned();
        }
    }
    if o.tex.mode == TexMode::Splat
        && o.tex.splat.trim().is_empty()
        && o.tex.channels.iter().all(|c| c.trim().is_empty())
    {
        return Err("texturing splat needs splat or splat_channels".to_owned());
    }
    Ok(o)
}

fn import_heightmap(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let opts = import_options(a)?;
    if a.opt_bool("preview")?.unwrap_or(false) {
        let field = height_field(&opts, state)?;
        let (tx, ty) = opts.setup.tiles();
        let sp = opts.setup.spacing();
        let min = opts.setup.min();
        return Ok(ToolResult::json(json!({
            "preview": true,
            "tiles": [tx, ty],
            "displacements": tx * ty,
            "vertex_spacing": [sp.x, sp.y],
            "image": [field.width, field.height],
            "min": [min.x, min.y],
            "size": [opts.setup.size.x, opts.setup.size.y],
        })));
    }
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    let report = import_as(state, &opts, &undo_label("import_heightmap"))?;
    Ok(ToolResult::json(json!({
        "tiles": report.tiles,
        "image": [report.image.0, report.image.1],
        "vertex_spacing": report.spacing,
    })))
}

fn export_heightmap(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = super::tools_disp::disp_targets(state, a, false)?;
    let path = a.str("path", MAX_PATH)?;
    safe_path(path)?;
    let splat = a.opt_str("splat_path", MAX_PATH)?.unwrap_or("");
    if !splat.is_empty() {
        safe_path(splat)?;
    }
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    let opts = ExportOptions {
        path: path.to_owned(),
        splat: splat.to_owned(),
        min_height: a.opt_f64("min_height", -16384.0, 16384.0)?,
        max_height: a.opt_f64("max_height", -16384.0, 16384.0)?,
    };
    if !overwrite {
        for p in std::iter::once(path).chain((!splat.is_empty()).then_some(splat)) {
            let full = hp_tools_mesh::terrain::resolve_path(state, p)?;
            if full.exists() {
                return Err(format!("{} exists (overwrite true replaces it)", full.display()));
            }
        }
    }
    let r = export(state, &faces, &opts)?;
    Ok(ToolResult::json(json!({
        "width": r.width,
        "height": r.height,
        "min_height": r.min_height,
        "max_height": r.max_height,
        "size_x": r.size.0,
        "size_y": r.size.1,
        "origin": [r.min.0, r.min.1, 0.0],
        "centered": false,
        "faces": r.faces,
        "splat": r.splat,
    })))
}

fn gen_options(a: &Args<'_>) -> R<GenOptions> {
    let mut o = GenOptions::default();
    let seed = a.opt_u64("seed", 0, u64::from(u32::MAX))?;
    if let Some(name) = a.opt_enum("preset", &TerrainPreset::ALL.map(TerrainPreset::name))? {
        o.apply_preset(TerrainPreset::from_name(&name).unwrap_or(TerrainPreset::RollingHills));
    }
    o.params.seed = seed.unwrap_or_else(random_seed);
    let p = &mut o.params;
    if let Some(v) = a.opt_enum("basis", &["perlin", "simplex"])? {
        p.basis = if v == "perlin" { NoiseBasis::Perlin } else { NoiseBasis::Simplex };
    }
    if let Some(v) = a.opt_enum("shape", &["plain", "ridged", "billow"])? {
        p.shape = match v.as_str() {
            "ridged" => NoiseShape::Ridged,
            "billow" => NoiseShape::Billow,
            _ => NoiseShape::Plain,
        };
    }
    let numbers: [(&str, f64, f64, &mut f64); 10] = [
        ("features", 0.1, 64.0, &mut p.frequency),
        ("persistence", 0.05, 1.0, &mut p.persistence),
        ("warp", 0.0, 2.0, &mut p.warp),
        ("stretch", 0.1, 16.0, &mut p.stretch),
        ("contrast", 0.1, 4.0, &mut p.contrast),
        ("curve", 0.2, 5.0, &mut p.exponent),
        ("sea_level", 0.0, 0.95, &mut p.sea_level),
        ("terrace_sharpness", 0.0, 1.0, &mut p.terrace_sharpness),
        ("falloff", 0.0, 1.0, &mut p.falloff),
        ("falloff_power", 0.1, 8.0, &mut p.falloff_power),
    ];
    for (key, lo, hi, slot) in numbers {
        if let Some(v) = a.opt_f64(key, lo, hi)? {
            *slot = v;
        }
    }
    if let Some(v) = a.opt_u64("octaves", 1, 10)? {
        p.octaves = v as u32;
    }
    if let Some(v) = a.opt_u64("terraces", 0, 64)? {
        p.terraces = v as u32;
    }
    if let Some(v) = a.opt_bool("invert_noise")? {
        p.invert = v;
    }
    if let Some(v) = a.opt_bool("island")? {
        p.island = v;
    }
    if let Some(v) = a.opt_enum("falloff_shape", &["round", "square"])? {
        p.falloff_shape = if v == "square" { FalloffShape::Square } else { FalloffShape::Round };
    }
    o.setup.h_max = 768.0;
    setup_from(a, &mut o.setup)?;
    let e = &mut o.erosion;
    e.thermal = a.opt_bool("thermal_erosion")?.unwrap_or(false);
    e.hydraulic = a.opt_bool("hydraulic_erosion")?.unwrap_or(false);
    if let Some(v) = a.opt_u64("thermal_iterations", 1, 1000)? {
        e.thermal_params.iterations = v as u32;
    }
    if let Some(v) = a.opt_f64("thermal_talus", 0.0, 89.0)? {
        e.thermal_params.talus_degrees = v;
    }
    if let Some(v) = a.opt_f64("thermal_strength", 0.0, 1.0)? {
        e.thermal_params.strength = v;
    }
    if let Some(v) = a.opt_u64("hydraulic_iterations", 1, u64::from(hp_geom::MAX_ITERATIONS))? {
        e.hydraulic_params.iterations = v as u32;
    }
    let h = &mut e.hydraulic_params;
    let hydraulic: [(&str, f64, &mut f64); 5] = [
        ("rain", 100.0, &mut h.rain),
        ("evaporation", 1.0, &mut h.evaporation),
        ("capacity", 10.0, &mut h.capacity),
        ("deposition", 1.0, &mut h.deposition),
        ("hydraulic_strength", 1.0, &mut h.strength),
    ];
    for (key, hi, slot) in hydraulic {
        if let Some(v) = a.opt_f64(key, 0.0, hi)? {
            *slot = v;
        }
    }
    o.auto_texture = a.opt_bool("auto_texture")?.unwrap_or(true);
    auto_from(a, &mut o.auto)?;
    Ok(o)
}

fn random_seed() -> u64 {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    u64::from(now.subsec_nanos() ^ (now.as_secs() as u32).wrapping_mul(2_654_435_761))
}

fn generate_terrain(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mut opts = gen_options(a)?;
    let replace = a.opt_bool("replace_selected")?.unwrap_or(false);
    opts.replace = replace;
    if !replace && ["faces", "ids", "solid_ids"].iter().any(|k| a.get(k).is_some()) {
        super::tools_disp::disp_targets(state, a, false)?;
    }
    match a.opt_enum("preview", &["show", "hide"])?.as_deref() {
        Some("hide") => {
            state.runtime.mesh.terrain_preview = None;
            return Ok(ToolResult::json(json!({ "preview": "hidden" })));
        }
        Some("show") => {
            let faces = if replace { super::tools_disp::disp_targets(state, a, false)? } else { Vec::new() };
            let version = state.runtime.mesh.terrain_preview.as_ref().map_or(1, |p| p.version + 1);
            let made = if replace {
                super::tools_disp::with_faces_selected(state, "generate_terrain", &faces, |s| {
                    preview(&s.doc, &opts, version)
                })?
            } else {
                preview(&state.doc, &opts, version)
            };
            let shown = made.is_some();
            state.runtime.mesh.terrain_preview = made;
            return Ok(ToolResult::json(json!({ "preview": "shown", "available": shown, "seed": opts.params.seed })));
        }
        _ => {}
    }
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    let label = undo_label("generate_terrain");
    let report = if replace {
        let faces = super::tools_disp::disp_targets(state, a, false)?;
        super::tools_disp::with_faces_selected(state, "generate_terrain", &faces, |s| generate_as(s, &opts, &label))??
    } else {
        generate_as(state, &opts, &label)?
    };
    state.runtime.mesh.terrain_preview = None;
    Ok(ToolResult::json(
        json!({ "seed": report.seed, "tiles": report.tiles, "faces": report.faces, "replaced": replace }),
    ))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn temp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("hp_mcp_terrain_{name}_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn small() -> serde_json::Value {
        json!({ "size_x": 1024, "size_y": 1024, "tile_size": 512, "tile_power": 2, "max_height": 300 })
    }

    fn merge(mut a: serde_json::Value, b: serde_json::Value) -> serde_json::Value {
        let (Some(x), Some(y)) = (a.as_object_mut(), b.as_object()) else { return a };
        for (k, v) in y {
            x.insert(k.clone(), v.clone());
        }
        a
    }

    #[test]
    fn generate_terrain_is_repeatable_undoable_and_reports_the_seed() {
        let mut s = hp_app::testing::test_state();
        let before = s.doc.history_len();
        let r = ok(&mut s, "generate_terrain", merge(small(), json!({ "preset": "mountains", "seed": 42 })));
        assert_eq!(r["seed"], 42);
        assert_eq!(r["tiles"], 4);
        assert_eq!(s.doc.disp_faces().len(), 4);
        assert_eq!(s.doc.history_len().0, before.0 + 1);
        let first: Vec<f64> = s
            .doc
            .disp_faces()
            .iter()
            .filter_map(|f| s.doc.scene().solid(f.solid)?.disp_mesh(f.side))
            .flat_map(|m| m.positions.iter().map(|p| p.z).collect::<Vec<_>>())
            .collect();
        ok(&mut s, "undo", json!({}));
        assert!(s.doc.disp_faces().is_empty());
        ok(&mut s, "generate_terrain", merge(small(), json!({ "preset": "mountains", "seed": 42 })));
        let second: Vec<f64> = s
            .doc
            .disp_faces()
            .iter()
            .filter_map(|f| s.doc.scene().solid(f.solid)?.disp_mesh(f.side))
            .flat_map(|m| m.positions.iter().map(|p| p.z).collect::<Vec<_>>())
            .collect();
        assert_eq!(first, second);
    }

    #[test]
    fn generate_terrain_replaces_selected_and_previews() {
        let mut s = hp_app::testing::test_state();
        ok(&mut s, "generate_terrain", merge(small(), json!({ "seed": 1 })));
        let r = ok(&mut s, "generate_terrain", merge(small(), json!({ "seed": 2, "replace_selected": true })));
        assert_eq!(r["faces"], 4);
        assert_eq!(s.doc.disp_faces().len(), 4);
        let shown = ok(&mut s, "generate_terrain", merge(small(), json!({ "seed": 3, "preview": "show" })));
        assert_eq!(shown["available"], true);
        assert!(s.runtime.mesh.terrain_preview.is_some());
        ok(&mut s, "generate_terrain", json!({ "preview": "hide" }));
        assert!(s.runtime.mesh.terrain_preview.is_none());
        let why = err(&mut s, "generate_terrain", json!({ "size_x": 100000 }));
        assert!(why.contains("size_x"), "{why}");
    }

    #[test]
    fn heightmap_round_trip_through_the_tools() {
        let d = temp("round");
        let mut s = hp_app::testing::test_state();
        ok(&mut s, "generate_terrain", merge(small(), json!({ "seed": 7, "centered": false, "auto_texture": false })));
        let path = d.join("h.png").to_string_lossy().into_owned();
        let splat = d.join("s.png").to_string_lossy().into_owned();
        let r = ok(&mut s, "export_heightmap", json!({ "path": path, "splat_path": splat }));
        assert_eq!(r["width"], 9);
        assert!(d.join("s.png").exists());
        let why = err(&mut s, "export_heightmap", json!({ "path": path }));
        assert!(why.contains("overwrite"), "{why}");
        ok(&mut s, "export_heightmap", json!({ "path": path, "overwrite": true }));
        let preview = ok(
            &mut s,
            "import_heightmap",
            json!({ "path": path, "preview": true, "size_x": 1024, "size_y": 1024, "tile_size": 512, "tile_power": 2 }),
        );
        assert_eq!(preview["tiles"], json!([2, 2]));
        assert_eq!(preview["image"], json!([9, 9]));
        let n = s.doc.disp_faces().len();
        let r = ok(
            &mut s,
            "import_heightmap",
            json!({
                "path": path, "size_x": 1024, "size_y": 1024, "tile_size": 512, "tile_power": 2,
                "origin": [r["origin"][0], r["origin"][1], 0], "centered": false,
                "min_height": r["min_height"], "max_height": r["max_height"],
                "texturing": "splat", "splat": splat,
            }),
        );
        assert_eq!(r["tiles"], 4);
        assert_eq!(s.doc.disp_faces().len(), n + 4);
    }

    #[test]
    fn bad_import_arguments_are_refused() {
        let mut s = hp_app::testing::test_state();
        let before = s.doc.history_len();
        assert!(err(&mut s, "import_heightmap", json!({ "path": "../x.png" })).contains("invalid path"));
        assert!(err(&mut s, "import_heightmap", json!({ "path": "x.png", "texturing": "splat" })).contains("splat"));
        assert!(
            err(&mut s, "import_heightmap", json!({ "path": "x.png", "max_height": 10, "height_scale": 5 }))
                .contains("not both")
        );
        assert!(err(&mut s, "import_heightmap", json!({ "path": "E:/no/such/file.png" })).contains("file.png"));
        assert_eq!(s.doc.history_len(), before);
    }
}
