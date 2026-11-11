use std::path::{Component, Path, PathBuf};

use hp_app::AppState;
use hp_app::state::ModelState;
use hp_mcp::protocol::ToolResult;
use hp_ui_textures::matedit::{self, ParamOp, VmtLocation};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{ApplyFn, Args, MAX_NAME, MAX_VALUE, Outcome, R, Worker, encode_png, material_name, v3};

const MAX_EDIT: usize = 64;
const MAX_IMPORT: usize = 16;
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;

domain! { "assets";
    search_sounds: Read, "Search sounds",
        "Search the sounds of the mounted game content by part of the path or a pattern with * and ?.",
        { "query": string("Text or pattern, for example \"ambient/water\"."), "limit": limit(), "offset": offset() }, ["query"];
    search_particles: Read, "Search particles",
        "Search the particle systems of the game content (name, file).",
        { "query": string("Text or pattern."), "limit": limit(), "offset": offset() }, ["query"];
    list_particle_systems: Read, "Particle systems of the map",
        "The info_particle_system entities of the map with their effect and how the particle preview of the 3D \
        views does with it: playing, loaded (not playing: too far, too many or not selected with \"selected \
        only\"), loading, not_found (no particle file of the content defines it), the particle file and the \
        operators the preview does not simulate. Also the preview settings (lab.show_particles, \
        lab.particles_selected_only, lab.show_entity_fx for the volume preview of func_smokevolume, env_embers, \
        func_dustcloud and func_dustmotes: change them with set_setting or the toggle_particles, \
        toggle_particles_selected and toggle_entity_fx actions), its caps and whether the particle preview window \
        (action particle_preview: it opens on the effect of the selected info_particle_system and draws it with the \
        same renderer as the 3D views) is open and which system it plays.",
        { "query": string("Only effects or target names containing this text."), "limit": limit(), "offset": offset() }, [];
    list_asset_sources: Read, "Content sources",
        "The mounted content sources (game folders, VPKs, addons, workshop GMAs, custom folders) and the custom \
        texture folders with their material prefixes.",
        {}, [];
    get_material_info: Read, "Material information",
        "A material's shader, texture size, base texture, surface property, tool kind, keywords and flags \
        (translucent, alpha test, water, blend, self illumination...).",
        { "material": string("Material name.") }, ["material"];
    get_model_info: Read, "Model information",
        "A model's bounds, skins, body groups, sequences, materials, triangles, surface property and whether it can \
        be a prop_static, plus shape: the exact size of its real mesh at angles 0 (bounds, how far the lowest point is \
        below the origin, footprint rectangle, collision mesh bounds, attachments). get_object_shape and \
        render_shape tell more. Models load in the background: call again while loading is true.",
        { "model": string("Model path (models/....mdl).") }, ["model"];
    get_material_preview: Read, "Material preview image",
        "A PNG image of a material's base texture (or of a texture), decoded at most size pixels on its longest \
        side, to see what it looks like.",
        { "material": string("Material name."), "texture": string("Texture name instead (a .vtf path)."), "size": integer("Longest side (default 256).", 16, 1024) }, [];
    read_material: Read, "Read a material",
        "The VMT text of a material and where it lives (file on disk, generated from an image, inside an archive).",
        { "material": string("Material name.") }, ["material"];
    edit_material: Edit, "Edit materials",
        "The material editor: set or remove VMT parameters of materials (comments and formatting kept, a .vmt.bak \
        backup first). Files must be in a custom texture folder; archived materials are copied into the custom \
        folder copy_into first.",
        {
            "materials": strings("Material names.", MAX_EDIT),
            "set": keyvalues("Parameters set: {\"$surfaceprop\": \"metal\"}."),
            "remove": strings("Parameters removed.", 64),
            "copy_into": integer("Custom folder index (list_asset_sources) for archived materials.", 0, 255),
        }, ["materials"];
    import_images: Edit, "Import images",
        "Turn images (PNG, JPG, TGA, BMP...) from the map folder or a custom texture folder into game textures \
        (VTF and VMT) of a custom texture folder.",
        {
            "files": strings("Image files (absolute, or relative to the map folder).", MAX_IMPORT),
            "folder": integer("Custom texture folder index (list_asset_sources).", 0, 255),
            "subfolder": string("Subfolder inside it."),
            "surface_prop": string("$surfaceprop of the materials."),
            "mipmaps": boolean("Generate mipmaps (default true)."),
            "overwrite": boolean("Replace existing textures (default false)."),
        }, ["files", "folder"];
    find_material_usage: Read, "Material usage",
        "Where materials matching a pattern are used: the solids and faces, and the entities naming them in keys \
        (overlays, decals, texture keys).",
        { "pattern": string("Material pattern with * and ?."), "limit": limit() }, ["pattern"];
    set_current_material: Editor, "Current material",
        "The material new brushes and Apply get (the texture browser's current material).",
        { "material": string("Material name.") }, ["material"];
    list_used_models: Read, "Models used by the map",
        "The models of the map's entities with their counts and classes, most used first.",
        { "limit": limit(), "offset": offset() }, [];
    get_uv_layers: Read, "UV layers of a material",
        "The layers of a material the UV editor edits: base (the face mapping), base2 (blend), bump, bump2 \
        (bump map of base2), detail, envmask and blendmod ($blendmodulatetexture), each with its texture, transform parameter and values (center, scale, rotate, translate, \
        $detailscale), and what the shader does with the transform: honoured, follows_base (the shader ignores \
        it: an edit is baked into a moved copy of the texture), not_drawn or unknown. Default material: the one \
        of the first selected face, else the current material.",
        { "material": string("Material name.") }, [];
    edit_uv_layer: Edit, "Edit a UV layer",
        "Move, scale or turn the base2, bump, bump2, detail, envmask or blendmod layer of a material like the UV editor, and apply \
        it to faces (apply_to faces: a variant of each face's material in the first custom folder, given to the \
        faces, one undo step) or to the material itself (apply_to material: its VMT in a custom folder, a backup \
        kept). Values not given keep the material's. When the shader ignores the layer transform (bump and detail \
        of LightmappedGeneric, base2 and bump of WorldVertexTransition...), the move is baked into a new copy of the texture (quarter turns, flips, whole \
        repeats, any shift). The base layer is the face mapping: set_face_texture, fit_texture, justify_texture.",
        {
            "layer": choice("Layer.", &["base2", "bump", "bump2", "detail", "envmask", "blendmod"]),
            "material": string("Material (default: the material of the first face, else of the first selected face, else the current one)."),
            "apply_to": choice("Where the edit goes (default faces).", &["faces", "material"]),
            "faces": faces("Faces getting the variant (default: the selected faces)."),
            "center": vec2("Center of the scale and rotation [u, v] (texture units, default 0.5 0.5)."),
            "scale": vec2("Scale [u, v]."),
            "rotate": number("Rotation in degrees.", -3600.0, 3600.0),
            "translate": vec2("Shift [u, v] in texture repeats."),
            "detail_scale": vec2("$detailscale [u, v] (detail layer)."),
        }, ["layer"];
    align_texture_edge: Edit, "Align or turn a texture on an edge",
        "Like the Edge mode of the UV editor: turn the texture of a face about the middle of its edge from vertex \
        `edge` to the next (the vertex order of get_faces), the scale kept: axis x or y lays the edge on the X (U) \
        or Y (V) axis of the texture (the smaller turn); angle gives the edge that direction on the texture \
        (degrees from the X axis, clockwise with V down, as the UV editor shows it); rotate_by turns the \
        texture by that many degrees about the middle of the edge. With fit_tiles (and an axis), scale it so \
        that many tiles span the edge, a tile corner on its start. The other faces given use their edge most \
        parallel to it (within 10 degrees). With a layer other than base (bump, detail...), the layer \
        transform of the material turns instead, applied to the faces as a variant like edit_uv_layer. One undo \
        step. Give one of axis, angle, rotate_by.",
        {
            "faces": faces("Faces, the first one with the edge (default: the selected faces)."),
            "edge": integer("Index of the first vertex of the edge.", 0, 255),
            "axis": choice("Axis of the texture the edge goes on.", &["x", "y"]),
            "angle": number("Direction of the edge on the texture, degrees from X (clockwise, V down).", -360.0, 360.0),
            "rotate_by": number("Turn of the texture about the middle of the edge, degrees (clockwise, V down).", -3600.0, 3600.0),
            "fit_tiles": number("Tiles along the edge (scales the texture; with axis).", 0.01, 1024.0),
            "layer": choice("Layer (default base: the face mapping).", &["base", "base2", "bump", "bump2", "detail", "envmask", "blendmod"]),
        }, ["edge"];
    move_texture_vertices: Edit, "Move vertices over a texture",
        "Like dragging vertices (or edges) in the Vertex and Edge modes of the UV editor: the given vertices of a \
        face move by delta texels over its texture, the others stay. A Source brush face (and a displacement, \
        through its base face) has one planar texture projection, not a UV per vertex, so this applies the \
        affine map (move, turn, scale, skew) closest to it: the moved vertices land exactly, the kept vertex \
        farthest from them (two with several moved) stays exactly when an affine map allows, the others move as \
        little as possible; refused when it would flatten the texture. The other faces given move the vertices \
        they share in the world with the moved ones. With a layer other than base, the layer under the vertices \
        follows instead (turn and scale, no skew), applied to the faces as a variant. One undo step. The result \
        lists the texels of every vertex of the first face before and after.",
        {
            "faces": faces("Faces, the first one with the vertices (default: the selected faces)."),
            "vertices": { "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 255 }, "maxItems": 256,
                "description": "Vertices moved (get_faces order)." },
            "edges": { "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 255 }, "maxItems": 256,
                "description": "Edges moved (index of the first vertex of each): both ends move." },
            "delta": vec2("Move [u, v] in texels of the base texture (V down on the canvas)."),
            "layer": choice("Layer (default base: the face mapping).", &["base", "base2", "bump", "bump2", "detail", "envmask", "blendmod"]),
        }, ["delta"];
    set_texture_origin: Edit, "Put a tile corner on a vertex",
        "Like the Vertex mode of the UV editor: shift the texture of a face so a corner of a texture tile lies on \
        its vertex `vertex` (the vertex order of get_faces); the other faces given that share the point get the \
        same. With a layer other than base, the layer transform moves instead (a variant of the faces). One undo \
        step.",
        {
            "faces": faces("Faces, the first one with the vertex (default: the selected faces)."),
            "vertex": integer("Index of the vertex.", 0, 255),
            "layer": choice("Layer (default base: the face mapping).", &["base", "base2", "bump", "bump2", "detail", "envmask", "blendmod"]),
        }, ["vertex"];
}

fn layer_material(state: &AppState, a: &Args<'_>, faces: &[hp_editor::FaceRef]) -> R<String> {
    if let Some(m) = a.opt_str("material", MAX_NAME)? {
        return material_name(m);
    }
    let face = faces.first().copied().or_else(|| state.doc.selection().faces.first().copied());
    if let Some(side) = face.and_then(|f| state.doc.face_side(f)) {
        return Ok(hp_ui_textures::util::normalize_material(&side.material));
    }
    let current = state.current_material.trim();
    if current.is_empty() {
        return Err("give material, or select a face".to_owned());
    }
    Ok(hp_ui_textures::util::normalize_material(current))
}

fn transform_json(t: &hp_ui_textures::uv_layers::Transform) -> Value {
    json!({ "center": t.center, "scale": t.scale, "rotate": t.rotate, "translate": t.translate, "value": t.format() })
}

fn get_uv_layers(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let material = layer_material(state, a, &[])?;
    let layers = hp_ui_textures::uv_layers::mcp_layers(state, &material)
        .ok_or_else(|| format!("no material {material} in the game content (search_materials)"))?;
    let list: Vec<Value> = layers
        .iter()
        .map(|l| {
            json!({
                "layer": l.layer.mcp_name(),
                "shader": l.shader,
                "support": l.support,
                "explanation": l.support_text,
                "texture": l.texture,
                "texture_key": l.texture_key,
                "baked_from": l.source,
                "transform_key": l.transform_key,
                "transform": transform_json(&l.transform),
                "detail_scale": (l.layer == hp_ui_textures::uv_layers::Layer::Detail).then_some(l.detail_scale),
                "edit_bakes_texture": l.bakes,
            })
        })
        .collect();
    Ok(ToolResult::json(json!({ "material": material, "layers": list })))
}

fn uv_pick(state: &mut AppState, a: &Args<'_>, tool: &str, pick: hp_ui_textures::uv_ops::Pick) -> R<ToolResult> {
    use hp_ui_textures::uv_layers::{Layer, LayerApplied, LayerTarget};
    use hp_ui_textures::uv_ops::Pick;
    let faces = target_faces(state, a, "faces")?;
    let first = *faces.first().ok_or("give faces, or select faces")?;
    let layer = a.opt_enum("layer", &["base", "base2", "bump", "bump2", "detail", "envmask", "blendmod"])?;
    let layer = Layer::from_mcp_name(layer.as_deref().unwrap_or("base")).ok_or("unknown layer")?;
    let label = super::tools::undo_label(tool);
    if layer != Layer::Base {
        let (material, edit) = hp_ui_textures::uv_ops::layer_pick(state, first, layer, &pick)?;
        if state.doc.in_transaction() {
            return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
        }
        let done = hp_ui_textures::uv_layers::mcp_apply_layer(
            state,
            &material,
            layer,
            &edit,
            &LayerTarget::Faces(faces),
            &label,
        )?;
        let LayerApplied::Variants(v) = done else { return Err("unexpected result".to_owned()) };
        return Ok(ToolResult::json(json!({
            "layer": layer.mcp_name(),
            "material": material,
            "variants": v.variants,
            "faces": v.faces,
            "baked_texture": v.baked,
            "transform": { "center": edit.center, "scale": edit.scale, "rotate": edit.rotate, "translate": edit.translate },
        })));
    }
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    let tag = format!("mcp_uv_pick:{}", label);
    let texels = |state: &AppState| hp_ui_textures::uv_ops::face_texels(state, first);
    let before = texels(state);
    state.doc.begin(label);
    let r = match &pick {
        Pick::Edge { edge, op } => hp_ui_textures::uv_ops::edges_op(state, &[(first, *edge)], &faces, *op, &tag),
        Pick::Vertex(i) => hp_ui_textures::uv_ops::vertex_origin(state, first, *i, &faces, &tag),
        Pick::Move { mask, delta } => {
            let masks = hp_ui_textures::uv_ops::shared_masks(state, &[(first, mask.clone())], &faces);
            let what = hp_ui_textures::uv_ops::labels::MOVE_VERTICES;
            hp_ui_textures::uv_ops::move_masks(state, &masks, glam::DVec2::new(delta[0], delta[1]), what, &tag)
        }
    };
    state.doc.commit();
    let n = r?;
    let after = texels(state);
    let sides: Vec<Value> = faces
        .iter()
        .filter_map(|f| {
            let s = state.doc.face_side(*f)?;
            Some(json!({
                "solid": f.solid, "side": f.side,
                "u_axis": [s.uaxis.axis.x, s.uaxis.axis.y, s.uaxis.axis.z], "u_shift": s.uaxis.shift, "u_scale": s.uaxis.scale,
                "v_axis": [s.vaxis.axis.x, s.vaxis.axis.y, s.vaxis.axis.z], "v_shift": s.vaxis.shift, "v_scale": s.vaxis.scale,
                "rotation": s.rotation,
            }))
        })
        .collect();
    Ok(ToolResult::json(json!({
        "layer": "base",
        "faces_changed": n,
        "faces": sides,
        "first_face_texels": { "before": before, "after": after },
    })))
}

fn align_texture_edge(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    use hp_ui_textures::uv_ops::{EdgeAxis, EdgeOp, Pick};
    let edge = a.opt_u64("edge", 0, 255)?.ok_or("edge is required")?;
    let axis = match a.opt_enum("axis", &["x", "y"])?.as_deref() {
        Some("y") => Some(EdgeAxis::Y),
        Some(_) => Some(EdgeAxis::X),
        None => None,
    };
    let angle = a.opt_f64("angle", -360.0, 360.0)?;
    let turn = a.opt_f64("rotate_by", -3600.0, 3600.0)?;
    let fit = a.opt_f64("fit_tiles", 0.01, 1024.0)?;
    let op = match (axis, angle, turn, fit) {
        (Some(axis), None, None, Some(n)) => EdgeOp::Fit(axis, n),
        (Some(axis), None, None, None) => EdgeOp::Align(axis),
        (None, Some(deg), None, None) => EdgeOp::Angle(deg),
        (None, None, Some(deg), None) => EdgeOp::Turn(deg),
        (None, _, _, Some(_)) => return Err("fit_tiles needs an axis".to_owned()),
        (None, None, None, None) => return Err("give axis, angle or rotate_by".to_owned()),
        _ => return Err("give only one of axis, angle, rotate_by".to_owned()),
    };
    let edge = usize::try_from(edge).map_err(|e| e.to_string())?;
    uv_pick(state, a, "align_texture_edge", Pick::Edge { edge, op })
}

fn move_texture_vertices(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let delta = opt_vec2(a, "delta", -1.0e5, 1.0e5)?.ok_or("delta is required")?;
    let faces = target_faces(state, a, "faces")?;
    let first = *faces.first().ok_or("give faces, or select faces")?;
    let n = state.doc.face_polygon(first).map_or(0, <[glam::DVec3]>::len);
    let indices = |key: &str| -> R<Vec<usize>> {
        let Some(v) = a.get(key) else { return Ok(Vec::new()) };
        let list = v.as_array().ok_or_else(|| format!("{key} must be a list of integers"))?;
        list.iter()
            .map(|x| {
                x.as_u64()
                    .and_then(|i| usize::try_from(i).ok())
                    .filter(|i| *i < n)
                    .ok_or_else(|| format!("{key}: the face has {n} vertices"))
            })
            .collect()
    };
    let mut mask = vec![false; n];
    for i in indices("vertices")? {
        mask[i] = true;
    }
    for i in indices("edges")? {
        mask[i] = true;
        mask[(i + 1) % n] = true;
    }
    if !mask.iter().any(|m| *m) {
        return Err("give vertices or edges".to_owned());
    }
    uv_pick(state, a, "move_texture_vertices", hp_ui_textures::uv_ops::Pick::Move { mask, delta: [delta.x, delta.y] })
}

fn set_texture_origin(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let vertex = a.opt_u64("vertex", 0, 255)?.ok_or("vertex is required")?;
    let vertex = usize::try_from(vertex).map_err(|e| e.to_string())?;
    uv_pick(state, a, "set_texture_origin", hp_ui_textures::uv_ops::Pick::Vertex(vertex))
}

fn opt_vec2_arr(a: &Args<'_>, key: &str, min: f64, max: f64) -> R<Option<[f64; 2]>> {
    Ok(opt_vec2(a, key, min, max)?.map(|v| [v.x, v.y]))
}

fn edit_uv_layer(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    use hp_ui_textures::uv_layers::{Layer, LayerApplied, LayerEdit, LayerTarget};
    let layer = a
        .opt_enum("layer", &["base2", "bump", "bump2", "detail", "envmask", "blendmod"])?
        .ok_or("layer is required")?;
    let layer = Layer::from_mcp_name(&layer).ok_or("unknown layer")?;
    let to_material = a.opt_enum("apply_to", &["faces", "material"])?.as_deref() == Some("material");
    let faces = match opt_faces(&state.doc, a, "faces")? {
        Some(_) if to_material => return Err("faces is for apply_to faces".to_owned()),
        Some(f) => f,
        None if to_material => Vec::new(),
        None => state.doc.selection().faces.iter().copied().collect(),
    };
    if !to_material && faces.is_empty() {
        return Err("give faces, or select faces (or apply_to material)".to_owned());
    }
    let material = layer_material(state, a, &faces)?;
    let edit = LayerEdit {
        center: opt_vec2_arr(a, "center", -1.0e4, 1.0e4)?,
        scale: opt_vec2_arr(a, "scale", -1.0e4, 1.0e4)?,
        rotate: a.opt_f64("rotate", -3600.0, 3600.0)?,
        translate: opt_vec2_arr(a, "translate", -1.0e4, 1.0e4)?,
        detail_scale: opt_vec2_arr(a, "detail_scale", -1.0e4, 1.0e4)?,
    };
    if edit == LayerEdit::default() {
        return Err("give center, scale, rotate, translate or detail_scale".to_owned());
    }
    if to_material {
        let fs = state.assets.fs().ok_or("the game content is not loaded")?;
        let path = fs.disk_path(&format!("materials/{material}.vmt"));
        if !path.as_deref().is_some_and(|p| inside(p, &custom_dirs(state))) {
            return Err(format!(
                "{material} is not a file of a custom texture folder: apply to faces (a variant), or copy it with \
                 edit_material copy_into first"
            ));
        }
    }
    let target = if to_material { LayerTarget::Material } else { LayerTarget::Faces(faces) };
    let label = super::tools::undo_label("edit_uv_layer");
    let done = hp_ui_textures::uv_layers::mcp_apply_layer(state, &material, layer, &edit, &target, &label)?;
    Ok(ToolResult::json(match done {
        LayerApplied::Variants(v) => json!({
            "applied_to": "faces",
            "material": material,
            "layer": layer.mcp_name(),
            "variants": v.variants,
            "faces": v.faces,
            "baked_texture": v.baked,
        }),
        LayerApplied::Material(m) => json!({
            "applied_to": "material",
            "material": material,
            "layer": layer.mcp_name(),
            "file": m.file.display().to_string(),
            "backup": m.backup.map(|b| b.display().to_string()),
            "written": m.params,
            "baked_texture": m.baked,
        }),
    }))
}

fn matches(query: &str, name: &str) -> bool {
    if query.contains(['*', '?']) { hp_editor::glob_match(query, name) } else { name.contains(query) }
}

fn query(a: &Args<'_>) -> R<String> {
    let q = a.str("query", MAX_NAME)?.trim().replace('\\', "/").to_ascii_lowercase();
    if q.is_empty() {
        return Err("query must not be empty".to_owned());
    }
    Ok(q)
}

fn search_sounds(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let q = query(a)?;
    let list = state.assets.sounds();
    let hits: Vec<&str> = list.iter().filter(|e| matches(&q, &e.name)).map(|e| e.name.as_str()).collect();
    let (hits, total) = page(hits, a)?;
    Ok(ToolResult::json(json!({ "total": total, "sounds": hits })))
}

fn search_particles(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let q = query(a)?;
    let Some(cat) = state.assets.particles() else {
        return Ok(ToolResult::json(
            json!({ "total": 0, "particles": [], "note": "the particle list is loading or the game content is not configured" }),
        ));
    };
    let hits: Vec<Value> = cat
        .systems
        .iter()
        .filter(|s| matches(&q, &s.name.to_ascii_lowercase()))
        .map(|s| json!({ "name": s.name, "file": s.file, "material": s.material }))
        .collect();
    let (hits, total) = page(hits, a)?;
    Ok(ToolResult::json(json!({ "total": total, "particles": hits })))
}

fn list_particle_systems(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let q = a.opt_str("query", MAX_NAME)?.map(|s| s.trim().to_ascii_lowercase()).unwrap_or_default();
    let report = &state.runtime.lab.particles;
    let settings = &state.settings.lab;
    let mut systems: Vec<Value> = Vec::new();
    for (id, node) in state.doc.scene().entities() {
        let e = &node.entity;
        if !e.classname.eq_ignore_ascii_case("info_particle_system") {
            continue;
        }
        let effect = e.get("effect_name").map(str::trim).unwrap_or("");
        let key = effect.to_ascii_lowercase();
        let name = e.get("targetname").unwrap_or("");
        if !q.is_empty() && !key.contains(&q) && !name.to_ascii_lowercase().contains(&q) {
            continue;
        }
        let status = if key.is_empty() {
            "no_effect"
        } else if !settings.show_particles {
            "preview_off"
        } else if report.playing.iter().any(|(pid, _)| pid == id) {
            "playing"
        } else if report.loaded.contains_key(&key) {
            "loaded"
        } else if report.missing.contains(&key) {
            "not_found"
        } else {
            "loading"
        };
        let points: Vec<Value> = (1..64)
            .filter_map(|k| {
                e.get(&format!("cpoint{k}"))
                    .filter(|v| !v.trim().is_empty())
                    .map(|v| json!({ "point": k, "target": v }))
            })
            .collect();
        systems.push(json!({
            "id": id,
            "targetname": name,
            "effect_name": effect,
            "origin": e.origin().map(v3),
            "start_active": e.get("start_active").is_some_and(|v| v.trim() == "1"),
            "control_points": points,
            "status": status,
            "file": report.loaded.get(&key),
            "unsupported_operators": report.unsupported.get(&key).cloned().unwrap_or_default(),
        }));
    }
    let (systems, total) = page(systems, a)?;
    Ok(ToolResult::json(json!({
        "total": total,
        "systems": systems,
        "preview": {
            "show_particles": settings.show_particles,
            "selected_only": settings.particles_selected_only,
            "volume_effects": settings.show_entity_fx,
            "volume_effects_max_entities": hp_ui_lab::entity_fx::MAX_ENTITIES,
            "volume_effects_max_particles": hp_ui_lab::entity_fx::MAX_PARTICLES,
            "preview_window_open": state.runtime.lab.particle_preview.open,
            "preview_window_system": state.runtime.lab.particle_preview.selected,
            "effects_playing": report.playing.len(),
            "particles": report.particles,
            "max_effects": hp_ui_lab::particles::MAX_EFFECTS,
            "max_particles": hp_ui_lab::particles::MAX_PARTICLES,
            "max_distance": hp_ui_lab::particles::MAX_DISTANCE,
        },
    })))
}

fn custom_folders(state: &AppState) -> Vec<Value> {
    state
        .settings
        .game
        .custom_texture_dirs
        .iter()
        .enumerate()
        .map(|(i, d)| json!({ "index": i, "path": d.path.display().to_string(), "prefix": d.prefix, "enabled": d.enabled, "include_models": d.include_models }))
        .collect()
}

fn list_asset_sources(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let sources: Vec<Value> = state
        .assets
        .fs()
        .map(|fs| {
            fs.sources()
                .into_iter()
                .take(2000)
                .map(|s| {
                    let kind = match s.kind {
                        hp_fs::SourceKind::Dir => "folder",
                        hp_fs::SourceKind::Vpk(_) => "vpk",
                        hp_fs::SourceKind::Gma(_) => "gma",
                        hp_fs::SourceKind::Custom(_) => "custom",
                    };
                    json!({ "id": s.id, "kind": kind, "label": s.label, "path": s.path.display().to_string(), "enabled": s.enabled })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(ToolResult::json(json!({
        "game_content": super::tools::asset_status(state),
        "sources": sources,
        "custom_texture_folders": custom_folders(state),
        "materials": state.assets.materials().len(),
        "models": state.assets.models().len(),
        "sounds": state.assets.sounds().len(),
    })))
}

fn get_material_info(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let name = material_name(a.str("material", MAX_NAME)?)?;
    let info = state.assets.material_info(&name).ok_or_else(|| format!("no material {name} in the game content"))?;
    Ok(ToolResult::json(json!({
        "material": info.name,
        "missing": info.missing,
        "width": info.width,
        "height": info.height,
        "shader": info.shader,
        "base_texture": info.base_texture,
        "surface_prop": info.surface_prop,
        "tool": info.tool.map(|t| format!("{t:?}").to_ascii_lowercase()),
        "translucent": info.translucent,
        "alpha_test": info.alpha_test,
        "see_through_alpha": info.shows_alpha,
        "additive": info.additive,
        "water": info.is_water,
        "blend": info.is_blend,
        "self_illum": info.self_illum,
        "env_map": info.env_map,
        "decal_scale": info.decal_scale,
        "keywords": info.keywords,
        "source": state.assets.source_label(info.source),
    })))
}

fn model_path(a: &Args<'_>) -> R<String> {
    let m = a.str("model", MAX_NAME)?.trim().replace('\\', "/").to_ascii_lowercase();
    if !m.starts_with("models/") || !m.ends_with(".mdl") || m.contains("..") || m.contains(':') {
        return Err("model must look like models/.../name.mdl".to_owned());
    }
    Ok(m)
}

fn get_model_info(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let path = model_path(a)?;
    match state.assets.model_info(&path) {
        ModelState::Loading => Ok(ToolResult::json(json!({ "model": path, "loading": true }))),
        ModelState::Failed(e) => Err(format!("{path}: {e}")),
        ModelState::Ready(m) => Ok(ToolResult::json(super::tools_place::with_model_shape(
            state.assets.fs(),
            &path,
            json!({
                "model": m.path,
                "loading": false,
                "bounds": { "min": v3(m.bounds_min), "max": v3(m.bounds_max) },
                "skins": m.skins,
                "body_parts": m.body_parts,
                "body_groups": m.body_groups.iter().map(|b| json!({ "name": b.name, "choices": b.choices })).collect::<Vec<_>>(),
                "triangles": m.triangles,
                "materials": m.materials,
                "static_prop": m.is_static_prop,
                "surface_prop": m.surface_prop,
                "sequences": m.sequences.iter().take(500).collect::<Vec<_>>(),
            }),
        ))),
    }
}

fn get_material_preview(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let size = a.opt_u64("size", 16, 1024)?.unwrap_or(256) as u32;
    let (texture, label) = match (a.opt_str("material", MAX_NAME)?, a.opt_str("texture", MAX_NAME)?) {
        (Some(m), None) => {
            let name = material_name(m)?;
            let info = state.assets.material_info(&name).ok_or_else(|| format!("no material {name}"))?;
            let base = info.base_texture.clone().ok_or_else(|| format!("{name} has no base texture"))?;
            (base, name)
        }
        (None, Some(t)) => {
            let t = material_name(t.trim().trim_end_matches(".vtf"))?;
            (t.clone(), t)
        }
        _ => return Err("give material or texture".to_owned()),
    };
    let fs = state.assets.fs().cloned().ok_or("the game content is not loaded")?;
    Ok(Worker::spawn("get_material_preview", move || {
        let tex = match fs.load_texture(&texture, Some(size)) {
            Ok(t) => t,
            Err(e) => return ToolResult::error(format!("get_material_preview: {texture}: {e}")),
        };
        match encode_png(tex.width, tex.height, tex.rgba) {
            Ok(png) => ToolResult::png(
                &png,
                format!(
                    "{label} ({}x{} texture, shown {}x{})",
                    tex.original_width, tex.original_height, tex.width, tex.height
                ),
            ),
            Err(e) => ToolResult::error(format!("get_material_preview: {e}")),
        }
    }))
}

fn location_json(l: &VmtLocation) -> Value {
    match l {
        VmtLocation::File(p) => json!({ "kind": "file", "path": p.display().to_string() }),
        VmtLocation::Virtual(p) => json!({ "kind": "generated", "path": p.display().to_string() }),
        VmtLocation::Archive(label) => json!({ "kind": "archive", "archive": label }),
        VmtLocation::Missing => json!({ "kind": "missing" }),
    }
}

fn read_material(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let name = material_name(a.str("material", MAX_NAME)?)?;
    let fs = state.assets.fs().cloned().ok_or("the game content is not loaded")?;
    Ok(Worker::spawn("read_material", move || {
        let m = matedit::load_vmt(&fs, &name);
        if matches!(m.location, VmtLocation::Missing) {
            return ToolResult::error(format!("read_material: no material {name}"));
        }
        let text: Option<String> = m.text.map(|t| t.chars().take(64 * 1024).collect());
        ToolResult::json(json!({ "material": m.name, "location": location_json(&m.location), "text": text }))
    }))
}

pub(super) fn custom_dirs(state: &AppState) -> Vec<PathBuf> {
    state.settings.game.custom_texture_dirs.iter().filter(|d| d.enabled).map(|d| d.path.clone()).collect()
}

fn resolved(p: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut cur = p.to_path_buf();
    loop {
        if let Ok(c) = std::fs::canonicalize(&cur) {
            return rest.into_iter().rev().fold(c, |acc, part| acc.join(part));
        }
        match (cur.file_name().map(std::ffi::OsStr::to_os_string), cur.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                cur = parent.to_path_buf();
            }
            _ => return p.to_path_buf(),
        }
    }
}

pub(super) fn inside(path: &Path, dirs: &[PathBuf]) -> bool {
    if path.components().any(|c| matches!(c, Component::ParentDir)) || !path.is_absolute() {
        return false;
    }
    if path.to_str().is_none_or(|text| super::semantic::local_path(text).is_err()) {
        return false;
    }
    let p = resolved(path);
    dirs.iter().filter(|d| d.is_absolute()).map(|d| resolved(d)).any(|d| p.starts_with(&d) && p != d)
}

fn edit_material(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let names: Vec<String> = a.strings("materials", MAX_EDIT)?.iter().map(|m| material_name(m)).collect::<R<_>>()?;
    if names.is_empty() {
        return Err("materials is empty".to_owned());
    }
    let mut ops: Vec<(String, ParamOp)> = Vec::new();
    for (k, v) in a.keyvalues("set")? {
        if v.len() > MAX_VALUE {
            return Err(format!("the value of {k} is too long"));
        }
        ops.push((k, ParamOp::Set(v)));
    }
    for k in a.strings("remove", 64)? {
        if k.is_empty() || k.len() > 63 || k.contains(['"', '{', '}', ' ', '\n']) {
            return Err(format!("invalid parameter name \"{k}\""));
        }
        ops.push((k, ParamOp::Remove));
    }
    if ops.is_empty() {
        return Err("give set or remove".to_owned());
    }
    let fs = state.assets.fs().cloned().ok_or("the game content is not loaded")?;
    let copy_into = match a.opt_u64("copy_into", 0, 255)? {
        Some(i) => {
            let d = state
                .settings
                .game
                .custom_texture_dirs
                .get(i as usize)
                .ok_or("copy_into: no custom folder has this index")?;
            if !d.enabled {
                return Err("copy_into: this custom folder is disabled".to_owned());
            }
            let source = fs.sources().into_iter().find(|s| s.path == d.path).map(|s| s.id);
            let layout =
                source.and_then(|id| fs.custom_dir_layout(id)).ok_or("copy_into: the folder is not mounted yet")?;
            Some((d.path.clone(), layout))
        }
        None => None,
    };
    let allowed = custom_dirs(state);
    Ok(Worker::spawn_then("edit_material", move || {
        let loaded: Vec<matedit::LoadedVmt> = names.iter().map(|n| matedit::load_vmt(&fs, n)).collect();
        let refs: Vec<&matedit::LoadedVmt> = loaded.iter().collect();
        let plan = matedit::plan(&refs, copy_into.as_ref().map(|(p, l)| (p.as_path(), l)));
        let mut results = Vec::new();
        let mut changed = Vec::new();
        for m in &loaded {
            if !plan.iter().any(|i| i.name == m.name) {
                let why = match m.location {
                    VmtLocation::Missing => "not found",
                    VmtLocation::Archive(_) => "inside an archive: give copy_into",
                    _ => "not editable",
                };
                results.push(json!({ "material": m.name, "error": why }));
            }
        }
        for item in plan {
            if !inside(&item.path, &allowed) {
                results.push(json!({ "material": item.name, "error": "the file is not in a custom texture folder" }));
                continue;
            }
            match matedit::apply_item(&item, &ops) {
                Ok(w) => {
                    if w.is_some() {
                        changed.push(item.new_name.clone());
                    }
                    results.push(json!({ "material": item.new_name, "changed": w.is_some(), "path": item.path.display().to_string() }));
                }
                Err(e) => results.push(json!({ "material": item.name, "error": e })),
            }
        }
        let apply: Option<ApplyFn> = (!changed.is_empty()).then(|| {
            let names = changed.clone();
            Box::new(move |state: &mut AppState| {
                for n in &names {
                    state.assets.invalidate_material(n);
                }
                None
            }) as ApplyFn
        });
        (ToolResult::json(json!({ "results": results })), apply)
    }))
}

fn image_path(state: &AppState, f: &str) -> R<PathBuf> {
    let p = super::semantic::local_path(f)?;
    let p = if p.is_absolute() {
        p
    } else {
        relative_file(f, "png")?;
        map_dir(state)?.join(f.trim().replace('\\', "/"))
    };
    let mut allowed = custom_dirs(state);
    if let Ok(d) = map_dir(state) {
        allowed.push(d);
    }
    if !inside(&p, &allowed) {
        return Err(format!("{f}: images are only read from the map folder and the custom texture folders"));
    }
    let meta = std::fs::metadata(&p).map_err(|_| format!("{f}: no such file"))?;
    if !meta.is_file() || meta.len() > MAX_IMAGE_BYTES {
        return Err(format!("{f}: not a file, or larger than 64 MB"));
    }
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or_default().to_ascii_lowercase();
    if !["png", "jpg", "jpeg", "tga", "bmp", "gif", "webp", "psd", "hdr", "exr", "tif", "tiff"].contains(&ext.as_str())
    {
        return Err(format!("{f}: not an image file"));
    }
    Ok(p)
}

fn import_images(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let files = a.strings("files", MAX_IMPORT)?;
    if files.is_empty() {
        return Err("files is empty".to_owned());
    }
    let paths: Vec<PathBuf> = files.iter().map(|f| image_path(state, f)).collect::<R<_>>()?;
    let index = a.opt_u64("folder", 0, 255)?.ok_or("folder is required")? as usize;
    let dir =
        state.settings.game.custom_texture_dirs.get(index).ok_or("folder: no custom texture folder has this index")?;
    if !dir.enabled {
        return Err("folder: this custom texture folder is disabled".to_owned());
    }
    let (mut dest, mut prefix) = (dir.path.clone(), dir.prefix.trim_matches('/').to_owned());
    if let Some(sub) = a.opt_str("subfolder", 128)? {
        let sub = relative_file(sub, "dir")?;
        let sub = sub.trim_end_matches(".dir").to_ascii_lowercase();
        dest = dest.join(&sub);
        prefix = if prefix.is_empty() { sub } else { format!("{prefix}/{sub}") };
    }
    let surface_prop = a.opt_str("surface_prop", 63)?.map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned);
    if surface_prop.as_deref().is_some_and(|s| !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
        return Err("surface_prop must be letters, digits and underscores".to_owned());
    }
    let opts = hp_app::state::ImportOptions {
        dest_dir: dest,
        prefix,
        surface_prop,
        generate_mipmaps: a.opt_bool("mipmaps")?.unwrap_or(true),
        overwrite: a.opt_bool("overwrite")?.unwrap_or(false),
    };
    let results = state.assets.import_images(&paths, &opts);
    let list: Vec<Value> = files
        .iter()
        .zip(results)
        .map(|(f, r)| match r {
            Ok(name) => json!({ "file": f, "material": name }),
            Err(e) => json!({ "file": f, "error": e }),
        })
        .collect();
    Ok(ToolResult::json(json!({ "results": list })))
}

fn find_material_usage(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let p = a.str("pattern", MAX_NAME)?.trim().replace('\\', "/").to_ascii_lowercase();
    if p.is_empty() {
        return Err("pattern must not be empty".to_owned());
    }
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(100) as usize;
    let scene = state.doc.scene();
    let mut solids: Vec<Value> = Vec::new();
    let mut solid_total = 0usize;
    let mut face_total = 0usize;
    let mut ids: Vec<u32> = scene.solids().keys().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let Some(n) = scene.solid(id) else { continue };
        let sides: Vec<usize> = n
            .solid
            .sides
            .iter()
            .enumerate()
            .filter(|(_, s)| hp_editor::glob_match(&p, &s.material.to_ascii_lowercase()))
            .map(|(i, _)| i)
            .collect();
        if sides.is_empty() {
            continue;
        }
        solid_total += 1;
        face_total += sides.len();
        if solids.len() < limit {
            solids.push(json!({ "solid": id, "sides": sides }));
        }
    }
    let mut entities: Vec<Value> = Vec::new();
    for (id, e) in scene.entities() {
        let keys: Vec<&str> = e
            .entity
            .props
            .iter()
            .filter(|(k, v)| {
                matches!(k.to_ascii_lowercase().as_str(), "material" | "texture" | "detailmaterial" | "ropematerial")
                    && hp_editor::glob_match(&p, &v.replace('\\', "/").to_ascii_lowercase())
            })
            .map(|(k, _)| k.as_str())
            .collect();
        if !keys.is_empty() && entities.len() < limit {
            entities.push(json!({ "id": id, "classname": e.entity.classname, "keys": keys }));
        }
    }
    Ok(ToolResult::json(
        json!({ "solids_total": solid_total, "faces_total": face_total, "solids": solids, "entities": entities }),
    ))
}

fn set_current_material(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let name = material_name(a.str("material", MAX_NAME)?)?;
    state.current_material = name.clone();
    let mut out = json!({ "material": name });
    if let Some(w) = super::tools::material_warning(state, &name) {
        out["warning"] = json!(w);
    }
    Ok(ToolResult::json(out))
}

fn list_used_models(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mut by_model: std::collections::HashMap<String, (usize, Vec<String>)> = std::collections::HashMap::new();
    for e in state.doc.scene().entities().values() {
        let Some(m) = e.entity.get("model").filter(|m| m.to_ascii_lowercase().ends_with(".mdl")) else { continue };
        let entry = by_model.entry(m.replace('\\', "/").to_ascii_lowercase()).or_default();
        entry.0 += 1;
        let c = e.entity.classname.to_ascii_lowercase();
        if !entry.1.contains(&c) {
            entry.1.push(c);
        }
    }
    let mut list: Vec<(String, (usize, Vec<String>))> = by_model.into_iter().collect();
    list.sort_by(|x, y| y.1.0.cmp(&x.1.0).then_with(|| x.0.cmp(&y.0)));
    let (list, total) = page(list, a)?;
    let out: Vec<Value> = list.into_iter().map(|(m, (n, c))| json!({ "model": m, "count": n, "classes": c })).collect();
    Ok(ToolResult::json(json!({ "total": total, "models": out })))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn asset_tools_on_mock_content() {
        let mut s = hp_app::testing::test_state();
        let r = ok(&mut s, "list_asset_sources", json!({}));
        assert!(r["custom_texture_folders"].is_array());
        let r = ok(&mut s, "get_material_info", json!({ "material": "materials/tools/toolsnodraw.vmt" }));
        assert_eq!(r["material"], "tools/toolsnodraw");
        assert_eq!(r["tool"], "nodraw");
        ok(&mut s, "set_current_material", json!({ "material": "Concrete\\ConcreteFloor001a" }));
        assert_eq!(s.current_material, "concrete/concretefloor001a");
        err(&mut s, "set_current_material", json!({ "material": "a b" }));
        ok(&mut s, "search_sounds", json!({ "query": "ambient/*" }));
        ok(&mut s, "search_particles", json!({ "query": "fire" }));
        err(&mut s, "get_model_info", json!({ "model": "../x.mdl" }));
        err(&mut s, "get_material_preview", json!({ "material": "concrete/concretefloor001a" }));
        err(&mut s, "get_material_preview", json!({}));
        err(&mut s, "read_material", json!({ "material": "concrete/concretefloor001a" }));
        err(&mut s, "edit_material", json!({ "materials": ["a"], "set": {} }));
        assert!(
            err(&mut s, "get_uv_layers", json!({ "material": "concrete/concretefloor001a" })).contains("no material")
        );
        err(&mut s, "edit_uv_layer", json!({ "layer": "bump", "rotate": 90 }));
        assert!(
            err(&mut s, "edit_uv_layer", json!({ "layer": "bump", "faces": [{ "solid": 2, "side": 0 }] }))
                .contains("give")
        );
        err(&mut s, "edit_uv_layer", json!({ "layer": "base", "faces": [{ "solid": 2, "side": 0 }], "rotate": 90 }));
        err(
            &mut s,
            "edit_uv_layer",
            json!({ "layer": "bump", "apply_to": "material", "faces": [{ "solid": 2, "side": 0 }], "rotate": 90 }),
        );
        assert!(
            err(&mut s, "edit_uv_layer", json!({ "layer": "detail", "material": "concrete/concretefloor001a", "apply_to": "material", "scale": [2, 2] }))
                .contains("game content")
        );
        assert!(
            err(
                &mut s,
                "edit_uv_layer",
                json!({ "layer": "bump", "faces": [{ "solid": 2, "side": 0 }], "rotate": 90 })
            )
            .contains("no material")
        );
        assert_eq!(s.doc.history_len().0, 0);
        let r = ok(
            &mut s,
            "align_texture_edge",
            json!({ "faces": [{ "solid": 7, "side": 0 }], "edge": 0, "axis": "x", "fit_tiles": 3 }),
        );
        assert_eq!(r["faces_changed"], 1);
        assert_eq!((s.doc.history_len().0, s.doc.undo_label()), (1, Some("MCP: align_texture_edge")));
        let r = ok(
            &mut s,
            "set_texture_origin",
            json!({ "faces": [{ "solid": 7, "side": 0 }, { "solid": 7, "side": 2 }], "vertex": 1 }),
        );
        assert!(r["faces_changed"].as_u64().is_some_and(|n| n >= 1));
        assert!(s.doc.history_len().0 <= 2);
        assert!(
            err(&mut s, "align_texture_edge", json!({ "faces": [{ "solid": 7, "side": 0 }], "edge": 9, "axis": "x" }))
                .contains("edge")
        );
        err(&mut s, "align_texture_edge", json!({ "faces": [{ "solid": 7, "side": 0 }], "edge": 0 }));
        assert!(
            err(
                &mut s,
                "set_texture_origin",
                json!({ "faces": [{ "solid": 7, "side": 0 }], "vertex": 0, "layer": "bump" })
            )
            .contains("material")
        );
        let same = |a: f64, b: f64| {
            let d = (a - b).rem_euclid(512.0);
            d < 1e-6 || 512.0 - d < 1e-6
        };
        let texel = |r: &serde_json::Value, when: &str, i: usize| {
            let t = &r["first_face_texels"][when][i];
            (t[0].as_f64().unwrap_or(f64::NAN), t[1].as_f64().unwrap_or(f64::NAN))
        };
        let one = json!([{ "solid": 7, "side": 0 }]);
        let r = ok(&mut s, "align_texture_edge", json!({ "faces": one, "edge": 1, "angle": 30 }));
        let ((x1, y1), (x2, y2)) = (texel(&r, "after", 1), texel(&r, "after", 2));
        let a = (y2 - y1).atan2(x2 - x1).to_degrees();
        assert!((a - 30.0).abs() < 1e-6, "{a}");
        assert_eq!(s.doc.undo_label(), Some("MCP: align_texture_edge"));
        let r = ok(&mut s, "align_texture_edge", json!({ "faces": one, "edge": 1, "rotate_by": 15 }));
        let ((x1, y1), (x2, y2)) = (texel(&r, "after", 1), texel(&r, "after", 2));
        assert!(((y2 - y1).atan2(x2 - x1).to_degrees() - 45.0).abs() < 1e-6);
        assert!(
            err(&mut s, "align_texture_edge", json!({ "faces": one, "edge": 1, "axis": "x", "angle": 3 }))
                .contains("only one")
        );
        assert!(err(&mut s, "align_texture_edge", json!({ "faces": one, "edge": 1, "fit_tiles": 2 })).contains("axis"));
        let r = ok(&mut s, "move_texture_vertices", json!({ "faces": one, "vertices": [2], "delta": [16, -8] }));
        assert_eq!(r["faces_changed"], 1);
        let (b2, a2) = (texel(&r, "before", 2), texel(&r, "after", 2));
        assert!(same(a2.0, b2.0 + 16.0) && same(a2.1, b2.1 - 8.0), "{b2:?} {a2:?}");
        let (b0, a0) = (texel(&r, "before", 0), texel(&r, "after", 0));
        assert!(same(a0.0, b0.0) && same(a0.1, b0.1), "{b0:?} {a0:?}");
        assert_eq!(s.doc.undo_label(), Some("MCP: move_texture_vertices"));
        let r = ok(&mut s, "move_texture_vertices", json!({ "faces": one, "edges": [0], "delta": [0, 24] }));
        for i in [0, 1] {
            let (b, a) = (texel(&r, "before", i), texel(&r, "after", i));
            assert!(same(a.0, b.0) && same(a.1, b.1 + 24.0), "{i}: {b:?} {a:?}");
        }
        assert!(err(&mut s, "move_texture_vertices", json!({ "faces": one, "delta": [1, 1] })).contains("vertices"));
        assert!(
            err(&mut s, "move_texture_vertices", json!({ "faces": one, "vertices": [9], "delta": [1, 1] }))
                .contains("vertices")
        );
        err(&mut s, "move_texture_vertices", json!({ "faces": one, "vertices": [0] }));
        while s.doc.undo() {}
        err(&mut s, "import_images", json!({ "files": ["C:/Windows/win.ini"], "folder": 0 }));
        let u = ok(&mut s, "find_material_usage", json!({ "pattern": "concrete/*" }));
        assert_eq!(u["faces_total"], 6);
        let r = ok(&mut s, "list_used_models", json!({}));
        assert!(r["total"].is_number());
    }

    #[test]
    fn particle_systems_of_the_map_with_their_status() {
        let mut s = hp_app::testing::test_state();
        let mut map = hp_vmf::Map::new_empty();
        for (id, effect, name) in
            [(10, "Fire_Small_01", "fire"), (11, "smoke_x", ""), (12, "ghost_fx", ""), (13, "new_fx", "")]
        {
            let mut e = hp_vmf::Entity { id, classname: "info_particle_system".into(), ..Default::default() };
            e.set("effect_name", effect);
            e.set("origin", "0 0 0");
            e.set("targetname", name);
            e.set("cpoint1", if id == 10 { "target" } else { "" });
            map.entities.push(e);
        }
        s.doc = hp_editor::Document::from_map(map);
        let report = &mut s.runtime.lab.particles;
        report.playing = vec![(10, "fire_small_01".into())];
        report.particles = 42;
        report.loaded.insert("fire_small_01".into(), "particles/fire_01.pcf".into());
        report.loaded.insert("smoke_x".into(), "particles/smoke.pcf".into());
        report.missing.insert("ghost_fx".into());
        report.unsupported.insert("smoke_x".into(), vec!["Cull when crossing plane".into()]);
        s.settings.lab.show_particles = true;
        let r = ok(&mut s, "list_particle_systems", json!({}));
        assert_eq!(r["total"], 4);
        let status: Vec<&str> =
            r["systems"].as_array().unwrap().iter().map(|v| v["status"].as_str().unwrap()).collect();
        assert_eq!(status, ["playing", "loaded", "not_found", "loading"]);
        assert_eq!(r["systems"][0]["file"], "particles/fire_01.pcf");
        assert_eq!(r["systems"][0]["control_points"][0]["target"], "target");
        assert_eq!(r["systems"][1]["unsupported_operators"][0], "Cull when crossing plane");
        assert_eq!(r["preview"]["particles"], 42);
        let r = ok(&mut s, "list_particle_systems", json!({ "query": "FIRE" }));
        assert_eq!(r["total"], 1);
        s.settings.lab.show_particles = false;
        let r = ok(&mut s, "list_particle_systems", json!({}));
        assert_eq!(r["systems"][0]["status"], "preview_off");
    }

    #[test]
    fn files_are_checked_against_the_allowed_folders() {
        let dir = std::env::temp_dir().join(format!("hp_mcp_assets_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("custom")).unwrap();
        std::fs::write(dir.join("custom/a.vmt"), "x").unwrap();
        let allowed = vec![dir.join("custom")];
        assert!(super::inside(&dir.join("custom/a.vmt"), &allowed));
        assert!(super::inside(&dir.join("custom/sub/new.vmt"), &allowed));
        assert!(!super::inside(&dir.join("other.vmt"), &allowed));
        assert!(!super::inside(&dir.join("custom/../other.vmt"), &allowed));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
