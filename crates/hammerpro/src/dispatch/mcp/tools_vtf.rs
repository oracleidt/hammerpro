use std::path::{Path, PathBuf};

use hp_app::AppState;
use hp_app::runtime::VtfRequest;
use hp_mcp::protocol::ToolResult;
use hp_ui_textures::vtfedit::store::{self, Kind};
use hp_vtf::ops::{
    AlphaSource, Channels, HeightSource, ImageFileFormat, NormalAlpha, NormalKernel, NormalOptions, ResizeFilter,
};
use hp_vtf::{EncodeQuality, ImageFormat, MipFilter, VtfDocument, flags};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{ApplyFn, Args, MAX_NAME, Outcome, R, Worker, encode_png};
use super::tools_assets::{custom_dirs, inside};

type RunOut = (Value, Option<(PathBuf, String, bool, bool)>);

const MAX_OPS: usize = 32;
const MAX_VTF_BYTES: u64 = 256 * 1024 * 1024;
const FORMAT_IDS: &[&str] = &[
    "dxt1", "dxt3", "dxt5", "ati1n", "ati2n", "bgra8888", "rgba8888", "abgr8888", "argb8888", "bgrx8888", "bgr888",
    "rgb888", "bgr565", "rgb565", "bgra4444", "bgra5551", "ia88", "i8", "a8", "uv88",
];
const MIP_FILTERS: &[&str] = &["flags", "color", "data", "normal"];
const ALPHA_MODES: &[&str] = &["opaque", "constant", "luminance", "red", "green", "blue", "invert"];

domain! { "vtf textures";
    get_vtf_info: Read, "VTF texture information",
        "Everything the VTF editor shows about a texture: where it lives (file, archive, generated from an image), \
        version, size, format, mip levels with their sizes, frames, faces (6 for a cubemap), volume slices, start \
        frame, flags (ids such as clamp_s, normal, no_lod, one_bit_alpha), reflectivity, bumpmap scale, thumbnail, \
        and whether the pixels can be edited. Give texture (a game texture name such as brick/brickwall001a) or \
        file (a .vtf in the map folder or a custom texture folder).",
        {
            "texture": string("Game texture name (materials/<name>.vtf without the extension)."),
            "file": string("A .vtf file: absolute, or relative to the map folder."),
        }, [];
    preview_vtf: Read, "VTF image view",
        "A PNG image of a VTF texture as the VTF editor shows it: choose the mip level, frame, cubemap face, volume \
        slice and the channels (rgba, rgb, r, g, b, a: a alone is shown as grayscale). The long edge is cut to size \
        pixels.",
        {
            "texture": string("Game texture name."),
            "file": string("A .vtf file (map folder or custom texture folder)."),
            "mip": integer("Mip level (0 is the full size).", 0, 16),
            "frame": integer("Frame of an animated texture.", 0, 65535),
            "face": integer("Face of a cubemap (0 right, 1 left, 2 back, 3 front, 4 up, 5 down).", 0, 6),
            "slice": integer("Slice of a volume texture.", 0, 8192),
            "channels": string("Channels shown: rgba (default), rgb, r, g, b, a, rg..."),
            "size": integer("Longest side in pixels (default 512).", 16, 2048),
        }, [];
    export_vtf: Edit, "Export a VTF to an image",
        "Write a texture as a PNG, TGA, BMP or JPG image file (the format follows the extension of path), in the map \
        folder or a custom texture folder. With all_frames, one file per frame and face is written \
        (name_f000_rt.png...). The image has the four channels, as stored in the chosen mip level.",
        {
            "texture": string("Game texture name."),
            "file": string("A .vtf file (map folder or custom texture folder)."),
            "path": string("Image file to write: absolute inside the map folder or a custom texture folder, or relative to the map folder."),
            "mip": integer("Mip level (default 0, the full size).", 0, 16),
            "frame": integer("Frame (default 0).", 0, 65535),
            "face": integer("Face (default 0).", 0, 6),
            "slice": integer("Slice of a volume texture (default 0).", 0, 8192),
            "all_frames": boolean("Write every frame and face."),
            "overwrite": boolean("Replace existing files (default false)."),
        }, ["path"];
    edit_vtf: Edit, "Edit or create a VTF texture",
        "The VTF editor without the window. Source: texture (a game texture), file (a .vtf) or image (a PNG, TGA, \
        JPG or BMP file to turn into a texture). Then ops, in order: set_flags (add, remove or set: flag ids from \
        get_vtf_info), set_format (dxt1, dxt3, dxt5, ati1n, ati2n, bgra8888, rgba8888, abgr8888, argb8888, \
        bgrx8888, bgr888, rgb888, bgr565, rgb565, bgra4444, bgra5551, ia88, i8, a8, uv88), set_quality (fast, \
        normal, best), resize (width, height, filter nearest, color, data or normal), resize_pow2 (round to the \
        nearest powers of two), regenerate_mipmaps (enabled, filter flags, color, data or normal), \
        normal_from_height (source luminance, red, green, blue, alpha, average; kernel simple, sobel3, sobel5; \
        scale; wrap; opengl; alpha opaque, keep or height; set_flag), set_alpha (mode opaque, constant with value, \
        luminance, red, green, blue or invert; or file: a grayscale image), set_bump_scale, set_start_frame, \
        set_reflectivity (value [r, g, b] or \"auto\"), set_version (1 to 5), set_cubemap (enabled), import_image \
        (file, frame, face), add_frame (optional file), remove_frame (index). Header only edits (flags, \
        bump scale, reflectivity, start frame) keep the stored pixels as they are; anything else rebuilds the \
        texture with mipmaps. The result is written into a custom texture folder (folder and name, or path), or in \
        place when the source already is a file in a custom folder; a .bak backup is kept; an existing other file \
        needs overwrite. create_material writes a VMT with $basetexture when none exists. dry_run only reports \
        the result.",
        {
            "texture": string("Game texture name."),
            "file": string("A .vtf file (map folder or custom texture folder)."),
            "image": string("An image file to make a new texture from."),
            "ops": json!({
                "type": "array",
                "maxItems": MAX_OPS,
                "items": { "type": "object" },
                "description": "Operations, for example {\"op\":\"set_format\",\"format\":\"dxt5\"} then {\"op\":\"set_flags\",\"add\":[\"clamp_s\",\"clamp_t\"]}.",
            }),
            "folder": integer("Custom texture folder index (list_asset_sources) to write into.", 0, 255),
            "name": string("Texture name inside the folder (letters, digits, _ - and /). Default: the name of the source texture."),
            "path": string("A .vtf file to write instead of folder and name: inside the map folder or a custom texture folder."),
            "overwrite": boolean("Replace a different existing file (default false)."),
            "create_material": boolean("Write a VMT with $basetexture when the material does not exist."),
            "shader": choice("Shader of that VMT (default LightmappedGeneric).", &["LightmappedGeneric", "UnlitGeneric", "VertexLitGeneric"]),
            "surface_prop": string("$surfaceprop of that VMT."),
            "dry_run": boolean("Do not write anything: report what the result would be."),
        }, ["ops"];
    open_vtf_editor: Editor, "Open the VTF editor",
        "Open a texture in the VTF editor window, or in the external editor (VTFEdit, VTFEdit++, VTFEdit Reloaded or \
        another installed VTF editor: see list_vtf_editors) with external: true. Without arguments the empty \
        window opens. With an external editor the texture is extracted when it is inside an archive, and saving \
        there copies it into a custom texture folder and reloads it in Hammer Pro. Shows what the user sees.",
        {
            "texture": string("Game texture name."),
            "file": string("A .vtf file, or an image to make a new texture from."),
            "external": boolean("Use the external editor instead of the built in one."),
        }, [];
    list_vtf_editors: Read, "External VTF editors",
        "The VTF editors installed on this computer (VTFEdit, VTFEdit++, VTFEdit Reloaded, the program registered \
        for .vtf files), the one in use, the setting vtf.prefer_integrated (set_setting) and the setting \
        vtf.editor_path (only the user changes it, in Settings, VTF editor).",
        { "refresh": boolean("Search again instead of using the first search.") }, [];
}

fn allowed_dirs(state: &AppState) -> Vec<PathBuf> {
    let mut v = custom_dirs(state);
    if let Ok(d) = map_dir(state) {
        v.push(d);
    }
    v
}

fn user_path(state: &AppState, p: &str) -> R<PathBuf> {
    let p = p.trim();
    let given = PathBuf::from(p);
    let path = if given.is_absolute() {
        given
    } else {
        relative_file(p, "x")?;
        map_dir(state)?.join(p.replace('\\', "/"))
    };
    if !inside(&path, &allowed_dirs(state)) {
        return Err(format!("{p}: files are only read and written in the map folder and the custom texture folders"));
    }
    Ok(path)
}

fn read_file(path: &Path) -> R<Vec<u8>> {
    let meta = std::fs::metadata(path).map_err(|_| format!("{}: no such file", path.display()))?;
    if !meta.is_file() || meta.len() > MAX_VTF_BYTES {
        return Err(format!("{}: not a file, or larger than 256 MB", path.display()));
    }
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

struct Given {
    bytes: Option<Vec<u8>>,
    image: Option<PathBuf>,
    name: Option<String>,
    disk: Option<PathBuf>,
    kind: &'static str,
    label: String,
}

fn given_source(state: &mut AppState, a: &Args<'_>, allow_image: bool) -> R<Given> {
    let texture = a.opt_str("texture", MAX_NAME)?;
    let file = a.opt_str("file", 255)?;
    let image = if allow_image { a.opt_str("image", 255)? } else { None };
    if usize::from(texture.is_some()) + usize::from(file.is_some()) + usize::from(image.is_some()) != 1 {
        return Err(if allow_image {
            "give exactly one of texture, file and image".to_owned()
        } else {
            "give exactly one of texture and file".to_owned()
        });
    }
    if let Some(t) = texture {
        let fs = state.assets.fs().cloned().ok_or("the game content is not loaded")?;
        let loc = store::locate(&fs, t);
        let kind = match loc.kind {
            Kind::Disk => "file",
            Kind::Archive => "archive",
            Kind::Generated => "generated",
            Kind::Missing => return Err(format!("no texture {t} in the game content (search_materials)")),
        };
        return Ok(Given {
            bytes: loc.bytes,
            image: loc.image_file,
            name: Some(loc.name),
            disk: loc.disk,
            kind,
            label: loc.label,
        });
    }
    if let Some(f) = file {
        let path = user_path(state, f)?;
        let bytes = read_file(&path)?;
        if !hp_vtf::is_vtf(&bytes) {
            return Err(format!("{f}: not a VTF file"));
        }
        let name = hp_ui_textures::vtfedit::texture_name_of(&path);
        return Ok(Given {
            bytes: Some(bytes),
            image: None,
            name,
            disk: Some(path),
            kind: "file",
            label: String::new(),
        });
    }
    let img = user_path(state, image.unwrap_or_default())?;
    if !img.is_file() {
        return Err(format!("{}: no such file", img.display()));
    }
    Ok(Given { bytes: None, image: Some(img), name: None, disk: None, kind: "image", label: String::new() })
}

fn load_image(path: &Path) -> R<hp_vtf::RgbaImage> {
    let meta = std::fs::metadata(path).map_err(|_| format!("{}: no such file", path.display()))?;
    if !meta.is_file() || meta.len() > 64 * 1024 * 1024 {
        return Err(format!("{}: not a file, or larger than 64 MB", path.display()));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    hp_vtf::load_image_file(&bytes, &ext).map_err(|e| e.to_string())
}

fn doc_of(g: &Given) -> R<VtfDocument> {
    match (&g.bytes, &g.image) {
        (Some(b), _) => VtfDocument::from_bytes(b).map_err(|e| e.to_string()),
        (None, Some(i)) => load_image(i).map(VtfDocument::from_image),
        _ => Err("nothing to read".to_owned()),
    }
}

pub(crate) fn info_json(bytes: &[u8]) -> R<Value> {
    let info = hp_vtf::read_info(bytes).map_err(|e| e.to_string())?;
    let mips: Vec<Value> = (0..info.mip_count)
        .map(|m| {
            let (w, h, d) = info.mip_dimensions(m);
            json!({ "level": m, "width": w, "height": h, "slices": d })
        })
        .collect();
    let editable = info.format.is_decodable() && info.depth == 1;
    Ok(json!({
        "version": format!("{}.{}", info.version.0, info.version.1),
        "width": info.width,
        "height": info.height,
        "power_of_two": info.width.is_power_of_two() && info.height.is_power_of_two(),
        "format": info.format.name().to_ascii_lowercase(),
        "has_alpha": info.format.has_alpha(),
        "mip_levels": info.mip_count,
        "mips": mips,
        "frames": info.frames,
        "start_frame": info.first_frame,
        "faces": info.faces,
        "cubemap": info.faces > 1,
        "slices": info.depth,
        "flags": { "value": info.flags, "ids": flags::names(info.flags) },
        "reflectivity": info.reflectivity,
        "bumpmap_scale": info.bumpmap_scale,
        "thumbnail": info.low_res_format.map(|f| json!({
            "width": info.low_res_width, "height": info.low_res_height, "format": f.name().to_ascii_lowercase(),
        })),
        "file_size": bytes.len(),
        "pixels_editable": editable,
        "encodable_format": hp_vtf::can_encode(info.format),
    }))
}

fn get_vtf_info(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let g = given_source(state, a, false)?;
    let Some(bytes) = &g.bytes else {
        return Err("this texture is generated from an image file: edit_vtf with image, or open it in the VTF editor"
            .to_owned());
    };
    let mut v = info_json(bytes)?;
    v["texture"] = json!(g.name);
    v["location"] = json!({
        "kind": g.kind,
        "path": g.disk.as_ref().map(|p| p.display().to_string()),
        "archive": (g.kind == "archive").then_some(g.label.clone()),
    });
    v["flag_ids"] = json!(flags::NAMED.iter().map(|(_, n)| *n).collect::<Vec<_>>());
    v["encode_formats"] = json!(FORMAT_IDS);
    Ok(ToolResult::json(v))
}

fn view_args(a: &Args<'_>) -> R<(u32, usize, usize, u32)> {
    let mip = a.opt_u64("mip", 0, 16)?.unwrap_or(0) as u32;
    let frame = a.opt_u64("frame", 0, 65535)?.unwrap_or(0) as usize;
    let face = a.opt_u64("face", 0, 6)?.unwrap_or(0) as usize;
    let slice = a.opt_u64("slice", 0, 8192)?.unwrap_or(0) as u32;
    Ok((mip, frame, face, slice))
}

fn preview_vtf(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let g = given_source(state, a, false)?;
    let bytes = g.bytes.ok_or("this texture is generated from an image file")?;
    let (mip, frame, face, slice) = view_args(a)?;
    let size = a.opt_u64("size", 16, 2048)?.unwrap_or(512) as u32;
    let channels = match a.opt_str("channels", 8)? {
        Some(c) => Channels::from_id(c).ok_or("channels must be letters among r, g, b and a")?,
        None => Channels::default(),
    };
    let img = hp_vtf::decode_slice_rgba8(&bytes, mip, frame as u32, face as u32, slice).map_err(|e| e.to_string())?;
    let (w, h) = (img.width, img.height);
    let shown = hp_vtf::ops::view_channels(&img, channels);
    let shown = if w.max(h) > size {
        let s = f64::from(size) / f64::from(w.max(h));
        hp_vtf::ops::resize(
            &shown,
            ((f64::from(w) * s) as u32).max(1),
            ((f64::from(h) * s) as u32).max(1),
            ResizeFilter::Color,
        )
        .map_err(|e| e.to_string())?
    } else {
        shown
    };
    let png = encode_png(shown.width, shown.height, shown.pixels)?;
    Ok(ToolResult::png(&png, format!("{}x{} mip {mip} frame {frame} face {face}", w, h)))
}

fn export_vtf(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let g = given_source(state, a, false)?;
    let bytes = g.bytes.ok_or("this texture is generated from an image file")?;
    let (mip, frame, face, slice) = view_args(a)?;
    let target = user_path(state, a.str("path", 255)?)?;
    let ext = target.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let format = ImageFileFormat::from_extension(&ext).ok_or("path must end in .png, .tga, .bmp or .jpg")?;
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    let info = hp_vtf::read_info(&bytes).map_err(|e| e.to_string())?;
    let mut pairs: Vec<(usize, usize, PathBuf)> = Vec::new();
    if a.opt_bool("all_frames")?.unwrap_or(false) {
        let stem = target.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let dir = target.parent().map(Path::to_path_buf).unwrap_or_default();
        for f in 0..info.frames as usize {
            for c in 0..info.faces as usize {
                let mut name = stem.clone();
                if info.frames > 1 {
                    name.push_str(&format!("_f{f:03}"));
                }
                if info.faces > 1 {
                    name.push_str(&format!(
                        "_{}",
                        ["rt", "lf", "bk", "ft", "up", "dn", "sph"].get(c).unwrap_or(&"face")
                    ));
                }
                pairs.push((f, c, dir.join(format!("{name}.{}", format.extension()))));
            }
        }
    } else {
        pairs.push((frame, face, target.clone()));
    }
    if !overwrite && let Some((_, _, p)) = pairs.iter().find(|(_, _, p)| p.exists()) {
        return Err(format!("{} already exists: give overwrite", p.display()));
    }
    let mut written = Vec::new();
    for (f, c, path) in pairs {
        let img = hp_vtf::decode_slice_rgba8(&bytes, mip, f as u32, c as u32, slice).map_err(|e| e.to_string())?;
        let data = hp_vtf::ops::encode_image_file(&img, format).map_err(|e| e.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))?;
        written.push(json!({ "path": path.display().to_string(), "width": img.width, "height": img.height, "frame": f, "face": c }));
    }
    Ok(ToolResult::json(json!({ "written": written })))
}

fn flag_list(op: &Value, key: &str) -> R<u32> {
    let mut bits = 0;
    let Some(v) = op.get(key) else { return Ok(0) };
    let list = v.as_array().ok_or_else(|| format!("{key} must be a list of flag ids"))?;
    for item in list {
        let id = item.as_str().ok_or_else(|| format!("{key} must be a list of flag ids"))?;
        bits |= flags::from_id(id).ok_or_else(|| {
            format!(
                "unknown flag \"{id}\" (flags: {})",
                flags::NAMED.iter().map(|(_, n)| *n).collect::<Vec<_>>().join(", ")
            )
        })?;
    }
    Ok(bits)
}

fn parse_format(id: &str) -> R<ImageFormat> {
    ImageFormat::ALL
        .iter()
        .copied()
        .find(|f| f.name().eq_ignore_ascii_case(id.trim()) && hp_vtf::ENCODABLE.contains(f))
        .ok_or_else(|| format!("unknown format \"{id}\" (formats: {})", FORMAT_IDS.join(", ")))
}

fn op_u32(op: &Value, key: &str, min: u32, max: u32) -> R<Option<u32>> {
    match op.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_u64()
            .filter(|n| (u64::from(min)..=u64::from(max)).contains(n))
            .map(|n| Some(n as u32))
            .ok_or_else(|| format!("{key} must be an integer from {min} to {max}")),
    }
}

fn op_f32(op: &Value, key: &str, min: f64, max: f64) -> R<Option<f32>> {
    match op.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite() && (min..=max).contains(n))
            .map(|n| Some(n as f32))
            .ok_or_else(|| format!("{key} must be a number from {min} to {max}")),
    }
}

fn op_str<'a>(op: &'a Value, key: &str) -> Option<&'a str> {
    op.get(key).and_then(Value::as_str)
}

fn op_enum<T>(op: &Value, key: &str, from: impl Fn(&str) -> Option<T>) -> R<Option<T>> {
    match op_str(op, key) {
        None => Ok(None),
        Some(s) => from(s).map(Some).ok_or_else(|| format!("unknown {key} \"{s}\"")),
    }
}

pub(crate) fn apply_ops(doc: &mut VtfDocument, ops: &[Value], resolve: &dyn Fn(&str) -> R<PathBuf>) -> R<Vec<Value>> {
    let mut report = Vec::new();
    for (i, op) in ops.iter().enumerate() {
        let name = op_str(op, "op").ok_or_else(|| format!("ops[{i}]: op is required"))?.to_ascii_lowercase();
        let fail = |e: String| format!("ops[{i}] {name}: {e}");
        let vt = |e: hp_vtf::VtfError| format!("ops[{i}] {name}: {e}");
        match name.as_str() {
            "set_flags" => {
                let (add, remove) = (flag_list(op, "add").map_err(fail)?, flag_list(op, "remove").map_err(fail)?);
                let set = flag_list(op, "set").map_err(fail)?;
                if op.get("set").is_some() {
                    doc.flags = set | (doc.flags & flags::ENVMAP);
                }
                doc.flags = (doc.flags | add) & !remove;
                doc.flags = (doc.flags & !flags::ENVMAP) | (if doc.face_count() > 1 { flags::ENVMAP } else { 0 });
                if (add | remove | set) & (flags::ONEBITALPHA | flags::EIGHTBITALPHA) != 0 {
                    doc.recompute_alpha_flags = false;
                }
            }
            "set_format" => {
                let f = parse_format(op_str(op, "format").ok_or_else(|| fail("format is required".to_owned()))?)
                    .map_err(fail)?;
                doc.set_format(f).map_err(vt)?;
            }
            "set_quality" => {
                doc.quality = match op_str(op, "value") {
                    Some("fast") => EncodeQuality::Fast,
                    Some("normal") => EncodeQuality::Normal,
                    Some("best") => EncodeQuality::Best,
                    _ => return Err(fail("value must be fast, normal or best".to_owned())),
                };
            }
            "resize" => {
                let w =
                    op_u32(op, "width", 1, 4096).map_err(fail)?.ok_or_else(|| fail("width is required".to_owned()))?;
                let h = op_u32(op, "height", 1, 4096)
                    .map_err(fail)?
                    .ok_or_else(|| fail("height is required".to_owned()))?;
                let f = op_enum(op, "filter", ResizeFilter::from_id).map_err(fail)?.unwrap_or_default();
                doc.resize_all(w, h, f).map_err(vt)?;
            }
            "resize_pow2" => {
                let f = op_enum(op, "filter", ResizeFilter::from_id).map_err(fail)?.unwrap_or_default();
                let (w, h) =
                    (hp_vtf::ops::nearest_power_of_two(doc.width()), hp_vtf::ops::nearest_power_of_two(doc.height()));
                doc.resize_all(w, h, f).map_err(vt)?;
            }
            "regenerate_mipmaps" => {
                let enabled = op.get("enabled").and_then(Value::as_bool).unwrap_or(true);
                let filter = match op_str(op, "filter") {
                    None | Some("flags") => MipFilter::FromFlags,
                    Some("color") => MipFilter::Color,
                    Some("data") => MipFilter::Data,
                    Some("normal") => MipFilter::Normal,
                    Some(_) => return Err(fail(format!("filter must be one of: {}", MIP_FILTERS.join(", ")))),
                };
                doc.regenerate_mipmaps(enabled, filter).map_err(vt)?;
            }
            "normal_from_height" => {
                let mut o = NormalOptions::default();
                if let Some(s) = op_enum(op, "source", HeightSource::from_id).map_err(fail)? {
                    o.source = s;
                }
                if let Some(k) = op_enum(op, "kernel", NormalKernel::from_id).map_err(fail)? {
                    o.kernel = k;
                }
                if let Some(s) = op_f32(op, "scale", 0.01, 100.0).map_err(fail)? {
                    o.scale = s;
                }
                if let Some(a) = op_enum(op, "alpha", NormalAlpha::from_id).map_err(fail)? {
                    o.alpha = a;
                }
                o.wrap = op.get("wrap").and_then(Value::as_bool).unwrap_or(true);
                o.opengl = op.get("opengl").and_then(Value::as_bool).unwrap_or(false);
                doc.map_images(|img| hp_vtf::ops::normal_from_height(img, &o)).map_err(vt)?;
                if op.get("set_flag").and_then(Value::as_bool).unwrap_or(true) {
                    doc.flags |= flags::NORMAL;
                    doc.flags &= !flags::SRGB;
                    if doc.format == ImageFormat::Dxt1 && o.alpha != NormalAlpha::Opaque {
                        doc.format = ImageFormat::Dxt5;
                    }
                }
            }
            "set_alpha" => {
                if let Some(file) = op_str(op, "file") {
                    let mask = load_image(&resolve(file)?).map_err(fail)?;
                    doc.map_images(|img| hp_vtf::ops::set_alpha_from_image(img, &mask)).map_err(vt)?;
                } else {
                    let mode = op_str(op, "mode").ok_or_else(|| fail("mode or file is required".to_owned()))?;
                    let value = op_u32(op, "value", 0, 255).map_err(fail)?.unwrap_or(255) as u8;
                    let source = AlphaSource::from_id(mode, value)
                        .ok_or_else(|| fail(format!("mode must be one of: {}", ALPHA_MODES.join(", "))))?;
                    doc.map_images(|img| Ok(hp_vtf::ops::set_alpha(img, source))).map_err(vt)?;
                }
            }
            "set_bump_scale" => {
                doc.bumpmap_scale = op_f32(op, "value", 0.0, 1000.0)
                    .map_err(fail)?
                    .ok_or_else(|| fail("value is required".to_owned()))?;
            }
            "set_start_frame" => {
                let v =
                    op_u32(op, "value", 0, 65534).map_err(fail)?.ok_or_else(|| fail("value is required".to_owned()))?;
                if v as usize >= doc.frame_count() {
                    return Err(fail(format!("the texture has {} frame(s)", doc.frame_count())));
                }
                doc.first_frame = v as u16;
            }
            "set_reflectivity" => match op.get("value") {
                Some(Value::String(s)) if s == "auto" => doc.auto_reflectivity = true,
                Some(Value::Array(list)) if list.len() == 3 => {
                    let mut c = [0f32; 3];
                    for (slot, v) in c.iter_mut().zip(list) {
                        *slot = v
                            .as_f64()
                            .filter(|f| (0.0..=1.0).contains(f))
                            .ok_or_else(|| fail("values are 0 to 1".to_owned()))?
                            as f32;
                    }
                    doc.reflectivity = c;
                    doc.auto_reflectivity = false;
                }
                _ => return Err(fail("value must be \"auto\" or [r, g, b]".to_owned())),
            },
            "set_version" => {
                let v = op_u32(op, "value", 1, 5).map_err(fail)?.ok_or_else(|| fail("value is required".to_owned()))?;
                doc.set_version(v).map_err(vt)?;
            }
            "set_cubemap" => {
                doc.set_cubemap(op.get("enabled").and_then(Value::as_bool).unwrap_or(true)).map_err(vt)?;
            }
            "import_image" => {
                let file = op_str(op, "file").ok_or_else(|| fail("file is required".to_owned()))?;
                let img = load_image(&resolve(file)?).map_err(fail)?;
                let frame = op_u32(op, "frame", 0, 65535).map_err(fail)?.unwrap_or(0) as usize;
                let face = op_u32(op, "face", 0, 6).map_err(fail)?.unwrap_or(0) as usize;
                let f = op_enum(op, "filter", ResizeFilter::from_id).map_err(fail)?.unwrap_or_default();
                doc.replace_image(frame, face, img, f).map_err(vt)?;
            }
            "add_frame" => {
                let img = match op_str(op, "file") {
                    Some(file) => Some(load_image(&resolve(file)?).map_err(fail)?),
                    None => None,
                };
                let f = op_enum(op, "filter", ResizeFilter::from_id).map_err(fail)?.unwrap_or_default();
                doc.add_frame(img, f).map_err(vt)?;
            }
            "remove_frame" => {
                let i2 =
                    op_u32(op, "index", 0, 65535).map_err(fail)?.ok_or_else(|| fail("index is required".to_owned()))?;
                doc.remove_frame(i2 as usize).map_err(vt)?;
            }
            other => return Err(format!("ops[{i}]: unknown op \"{other}\"")),
        }
        report.push(json!({ "op": name, "ok": true }));
    }
    Ok(report)
}

fn edit_vtf(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let g = given_source(state, a, true)?;
    let ops: Vec<Value> = a.get("ops").and_then(Value::as_array).cloned().ok_or("ops must be a list")?;
    let dry = a.opt_bool("dry_run")?.unwrap_or(false);
    let overwrite = a.opt_bool("overwrite")?.unwrap_or(false);
    let allowed = allowed_dirs(state);
    let customs = custom_dirs(state);
    let folder = a.opt_u64("folder", 0, 255)?;
    let name_arg = a.opt_str("name", 200)?;
    let path_arg = a.opt_str("path", 255)?;
    let target: Option<store::Target> = match (folder, path_arg) {
        (Some(_), Some(_)) => return Err("give folder and name, or path, not both".to_owned()),
        (Some(idx), None) => {
            let n = name_arg.map(str::to_owned).or_else(|| g.name.clone()).ok_or("name is required")?;
            Some(store::custom_target(state, idx as usize, &n)?)
        }
        (None, Some(p)) => {
            let mut path = user_path(state, p)?;
            if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("vtf")) {
                path.set_extension("vtf");
            }
            let name = hp_ui_textures::vtfedit::texture_name_of(&path).unwrap_or_default();
            Some(store::Target { path, name })
        }
        (None, None) => match (&g.disk, &g.name) {
            (Some(p), name) => {
                if !(inside(p, &customs) || g.kind == "file" && inside(p, &allowed)) {
                    return Err("the texture is a game file: give folder and name (or path) to write a copy into a custom texture folder".to_owned());
                }
                Some(store::Target { path: p.clone(), name: name.clone().unwrap_or_default() })
            }
            (None, _) if dry => None,
            (None, _) => return Err("give folder (and name), or path, to say where the texture is written".to_owned()),
        },
    };
    if let Some(t) = &target
        && !dry
        && !inside(&t.path, &allowed)
    {
        return Err(format!(
            "{}: textures are only written in the map folder and the custom texture folders",
            t.path.display()
        ));
    }
    let in_place = target.as_ref().zip(g.disk.as_ref()).is_some_and(|(t, d)| t.path == *d);
    if let Some(t) = &target
        && t.path.exists()
        && !in_place
        && !overwrite
        && !dry
    {
        return Err(format!("{} already exists: give overwrite", t.path.display()));
    }
    let create_material = a.opt_bool("create_material")?.unwrap_or(false);
    let shader = a.opt_enum("shader", &["lightmappedgeneric", "unlitgeneric", "vertexlitgeneric"])?;
    let shader = match shader.as_deref() {
        Some("unlitgeneric") => "UnlitGeneric",
        Some("vertexlitgeneric") => "VertexLitGeneric",
        _ => "LightmappedGeneric",
    };
    let surface_prop = a.opt_str("surface_prop", 63)?.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    if surface_prop.as_deref().is_some_and(|s| !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
        return Err("surface_prop must be letters, digits and underscores".to_owned());
    }
    let map = map_dir(state).ok();
    let resolve_dirs = allowed.clone();
    Ok(Worker::spawn_then("edit_vtf", move || {
        let resolve = |f: &str| -> R<PathBuf> {
            let given = PathBuf::from(f.trim());
            let p = if given.is_absolute() {
                given
            } else {
                map.clone().ok_or("save the map first")?.join(f.trim().replace('\\', "/"))
            };
            if inside(&p, &resolve_dirs) {
                Ok(p)
            } else {
                Err(format!("{f}: files are only read in the map folder and the custom texture folders"))
            }
        };
        let run = || -> R<RunOut> {
            let mut doc = doc_of(&g)?;
            let report = apply_ops(&mut doc, &ops, &resolve)?;
            let bytes = doc.to_bytes().map_err(|e| e.to_string())?;
            let rebuilt = doc.pixels_dirty || doc.rebuild;
            let mut out = json!({
                "operations": report,
                "rebuilt": rebuilt,
                "info": info_json(&bytes)?,
                "dry_run": dry,
            });
            let Some(t) = target else { return Ok((out, None)) };
            out["texture"] = json!(t.name);
            out["path"] = json!(t.path.display().to_string());
            if dry {
                return Ok((out, None));
            }
            let written =
                store::write_bytes_with_backup(&t.path, &bytes).map_err(|e| format!("{}: {e}", t.path.display()))?;
            out["backup"] = json!(written.backup.as_ref().map(|b| b.display().to_string()));
            let mut vmt_made = false;
            if create_material && !t.name.is_empty() {
                let plan = store::MaterialPlan {
                    shader: shader.to_owned(),
                    surface_prop: surface_prop.clone(),
                    translucent: doc.flags & flags::EIGHTBITALPHA != 0 && doc.flags & flags::ONEBITALPHA == 0,
                    alphatest: doc.flags & flags::ONEBITALPHA != 0,
                };
                match store::create_vmt(&t, &plan, false)? {
                    Some(p) => {
                        out["material_written"] = json!(p.display().to_string());
                        vmt_made = true;
                    }
                    None => out["material_written"] = Value::Null,
                }
            }
            Ok((out, Some((t.path, t.name, written.created, vmt_made))))
        };
        match run() {
            Ok((out, wrote)) => {
                let apply: Option<ApplyFn> = wrote.map(|(path, name, created, vmt)| {
                    Box::new(move |state: &mut AppState| {
                        store::refresh(state, &path, &name, &[]);
                        if vmt || created {
                            state.assets.remount(&state.settings);
                        }
                        None
                    }) as ApplyFn
                });
                (ToolResult::json(out), apply)
            }
            Err(e) => (ToolResult::error(format!("edit_vtf: {e}")), None),
        }
    }))
}

fn open_vtf_editor(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let texture = a.opt_str("texture", MAX_NAME)?;
    let file = a.opt_str("file", 255)?;
    if texture.is_some() && file.is_some() {
        return Err("give texture or file, not both".to_owned());
    }
    let external = a.opt_bool("external")?;
    if external == Some(true) && hp_app::vtf_external::resolve(&state.settings.vtf.editor_path).is_none() {
        return Err(
            "no external VTF editor found (list_vtf_editors): the user can set one in Settings, VTF editor".to_owned()
        );
    }
    let mut req = VtfRequest { external, ..VtfRequest::default() };
    if let Some(t) = texture {
        let fs = state.assets.fs().cloned().ok_or("the game content is not loaded")?;
        let loc = store::locate(&fs, t);
        if loc.kind == Kind::Missing {
            return Err(format!("no texture {t} in the game content (search_materials)"));
        }
        req.texture = Some(loc.name);
    }
    if let Some(f) = file {
        let path = user_path(state, f)?;
        if !path.is_file() {
            return Err(format!("{f}: no such file"));
        }
        req.file = Some(path);
    }
    state.runtime.vtf.open(req);
    if texture.is_none() && file.is_none() {
        state.runtime.vtf.open_window();
    }
    Ok(ToolResult::json(json!({
        "open": true,
        "external": external.unwrap_or(false),
        "note": "The window opens on the next frame; use get_vtf_info and preview_vtf to read the texture.",
    })))
}

fn list_vtf_editors(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let refresh = a.opt_bool("refresh")?.unwrap_or(false);
    let configured = state.settings.vtf.editor_path.clone();
    let prefer = state.settings.vtf.prefer_integrated;
    Ok(Worker::spawn("list_vtf_editors", move || {
        let found = if refresh { hp_app::vtf_external::rescan() } else { hp_app::vtf_external::detect() };
        let in_use = hp_app::vtf_external::resolve(&configured);
        ToolResult::json(json!({
            "detected": found.iter().map(|e| json!({ "label": e.label, "path": e.path.display().to_string() })).collect::<Vec<_>>(),
            "configured_path": configured,
            "in_use": in_use.map(|e| json!({ "label": e.label, "path": e.path.display().to_string() })),
            "prefer_integrated": prefer,
            "integrated_editor": true,
        }))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> hp_vtf::RgbaImage {
        let px: Vec<u8> = (0..w * h).flat_map(|i| [(i % w * 8) as u8, (i / w * 8) as u8, 90, 255]).collect();
        hp_vtf::RgbaImage::new(w, h, px).unwrap()
    }

    fn no_files(_: &str) -> R<PathBuf> {
        Err("no files in tests".to_owned())
    }

    #[test]
    fn ops_change_the_document() {
        let mut doc = VtfDocument::from_image(gradient(32, 32));
        let ops = vec![
            json!({ "op": "set_format", "format": "dxt5" }),
            json!({ "op": "resize", "width": 16, "height": 8, "filter": "color" }),
            json!({ "op": "set_flags", "add": ["clamp_s", "clamp_t", "no_lod"] }),
            json!({ "op": "set_bump_scale", "value": 3.0 }),
            json!({ "op": "regenerate_mipmaps", "enabled": false }),
            json!({ "op": "set_alpha", "mode": "constant", "value": 100 }),
        ];
        let report = apply_ops(&mut doc, &ops, &no_files).unwrap();
        assert_eq!(report.len(), 6);
        let bytes = doc.to_bytes().unwrap();
        let info = hp_vtf::read_info(&bytes).unwrap();
        assert_eq!((info.width, info.height, info.mip_count, info.format), (16, 8, 1, ImageFormat::Dxt5));
        assert_eq!(info.bumpmap_scale, 3.0);
        assert!(info.flags & flags::CLAMPS != 0 && info.flags & flags::NOLOD != 0);
        assert!(info.flags & flags::EIGHTBITALPHA != 0);
        let j = info_json(&bytes).unwrap();
        assert_eq!(j["format"], "dxt5");
        assert!(j["flags"]["ids"].as_array().unwrap().iter().any(|v| v == "clamp_s"));
    }

    #[test]
    fn normal_maps_and_errors() {
        let mut doc = VtfDocument::from_image(gradient(16, 16));
        apply_ops(&mut doc, &[json!({ "op": "normal_from_height", "scale": 4.0, "wrap": false })], &no_files).unwrap();
        assert!(doc.flags & flags::NORMAL != 0);
        let bytes = doc.to_bytes().unwrap();
        let img = hp_vtf::decode_rgba8(&bytes, &hp_vtf::DecodeOptions::default()).unwrap();
        assert!(img.pixels[2] > 150, "mostly blue: {:?}", &img.pixels[..4]);
        for bad in [
            json!({ "op": "set_format", "format": "dxt9" }),
            json!({ "op": "set_flags", "add": ["nope"] }),
            json!({ "op": "resize", "width": 0, "height": 4 }),
            json!({ "op": "teleport" }),
            json!({ "op": "set_reflectivity", "value": [2, 0, 0] }),
            json!({ "op": "set_alpha", "file": "a.png" }),
            json!({ "noop": 1 }),
        ] {
            assert!(apply_ops(&mut doc, std::slice::from_ref(&bad), &no_files).is_err(), "{bad}");
        }
    }

    #[test]
    fn header_only_edits_keep_pixels() {
        let bytes = hp_vtf::encode(&gradient(16, 16), &hp_vtf::EncodeOptions::default()).unwrap();
        let mut doc = VtfDocument::from_bytes(&bytes).unwrap();
        apply_ops(
            &mut doc,
            &[
                json!({ "op": "set_flags", "add": ["srgb"] }),
                json!({ "op": "set_reflectivity", "value": [0.2, 0.3, 0.4] }),
            ],
            &no_files,
        )
        .unwrap();
        let out = doc.to_bytes().unwrap();
        assert_eq!(out.len(), bytes.len());
        assert_eq!(hp_vtf::read_info(&out).unwrap().reflectivity, [0.2, 0.3, 0.4]);
        assert_eq!(&out[80..], &bytes[80..]);
    }

    #[test]
    fn tools_validate_arguments() {
        let mut s = hp_app::testing::test_state();
        use super::super::semantic_tests::{err, ok};
        err(&mut s, "get_vtf_info", json!({}));
        err(&mut s, "get_vtf_info", json!({ "texture": "a", "file": "b.vtf" }));
        err(&mut s, "get_vtf_info", json!({ "file": "C:/Windows/win.ini" }));
        err(&mut s, "edit_vtf", json!({ "ops": [] }));
        err(&mut s, "edit_vtf", json!({ "image": "a.png", "ops": [{ "op": "set_format", "format": "dxt1" }] }));
        err(&mut s, "export_vtf", json!({ "texture": "x/y", "path": "out.png" }));
        err(&mut s, "open_vtf_editor", json!({ "texture": "a", "file": "b.vtf" }));
        ok(&mut s, "open_vtf_editor", json!({}));
        assert!(s.runtime.vtf.show);
    }
}
