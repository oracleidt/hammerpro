use glam::DVec3;
use hp_app::AppState;
use hp_editor::FaceRef;
use hp_editor::mesh_disp::{
    self, AxisChoice, ConformOptions, Fill, MeshDispOptions, MeshDispResult, Origin, Pick, PowerChoice, TilePlan,
    TileSelectionOptions, Tri,
};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{ApplyFn, Args, Outcome, R, Worker, edit};
use crate::dispatch::mesh_disp::source::{self, SourceMesh, UpChoice};

const MAX_LISTED: usize = 500;

domain! { "mesh to displacement";
    mesh_to_displacement: Edit, "Mesh to displacement",
        "Turn a 3D mesh into a grid of displacement tiles, like Tools > Mesh to displacement. The mesh is an OBJ, FBX, glTF or GLB file (path, full or relative to the map folder), a model prop of the map (prop, an entity id) or a game model (model, like models/props_c17/oildrum001.mdl). It is projected along axis (z top down, x, y or best for the axis it faces most; flip projects from the other side): a ray at every displacement vertex takes the top surface (pick top) or the highest surface facing the ray (pick first), and the vertex is set to that height, so the tiles share their seams. Tiles that the mesh barely touches are skipped; margin keeps extra tiles around and fill (nearest or base) sets the vertices the mesh does not cover. Overhangs keep the top surface and are reported. power is auto (from the mesh detail), 2, 3 or 4. origin places the mesh: center (its middle and lowest point at position), min_corner or keep (default center for files, keep for props and models). skirt closes the border with walls down to base_height (default one thickness under the lowest point) and bottom adds a flat brush there. Files read: up (file, y or z) and convert_units (meters to Source units, default true). dry_run only returns the layout. One undo step.",
        {
            "path": string("OBJ, FBX, glTF or GLB file: full path, or relative to the map folder."),
            "prop": integer("Id of a prop_static or prop_physics entity to use as the mesh.", 1, i64::from(u32::MAX)),
            "model": string("Game model path ending in .mdl, placed at the origin."),
            "up": choice("Up axis of a file (default file).", &["file", "y", "z"]),
            "convert_units": boolean("Meters of a file become Source units (default true)."),
            "axis": choice("Projection axis (default z).", &["z", "x", "y", "best"]),
            "flip": boolean("Project from the other side (default false)."),
            "tile_size": number("Tile width in units (default 256).", 16.0, 4096.0),
            "power": choice("Displacement power (default auto).", &["auto", "2", "3", "4"]),
            "pick": choice("Surface taken under the ray (default top).", &["top", "first"]),
            "fill": choice("Vertices the mesh does not cover (default nearest).", &["nearest", "base"]),
            "margin": integer("Extra tiles kept around the mesh (default 0).", 0, 8),
            "smooth": integer("Smoothing iterations of the heights (default 0).", 0, 64),
            "scale": number("Scale of the mesh after the unit conversion (default 1).", 0.001, 1000.0),
            "origin": choice("Where the mesh goes.", &["center", "min_corner", "keep"]),
            "position": vec3("World position used by origin center and min_corner (default 0 0 0)."),
            "material": string("Texture of the tiles (default dev/dev_measuregeneric01b)."),
            "uv_scale": number("Texture scale (default 0.25).", 0.001, 64.0),
            "skirt": boolean("Close the border with walls down to the base height (default false)."),
            "bottom": boolean("Add a flat brush under the tiles at the base height (default false)."),
            "base_height": number("Height of the skirt bottoms and the flat brush along the axis (default one thickness under the lowest point).", -131_072.0, 131_072.0),
            "thickness": number("Thickness of the brushes under the displacements (default 16).", 1.0, 4096.0),
            "group": boolean("Group the new brushes (default true)."),
            "dry_run": boolean("Only return the layout, change nothing (default false)."),
        }, [];
    conform_displacements_to_mesh: Edit, "Conform displacements to mesh",
        "Project existing displacements onto a 3D mesh: every vertex is moved along axis to the mesh surface (distances or offsets are set so the surface matches), so flat displacement tiles placed by hand can be shaped by a mesh. The mesh is a file (path), a model prop (prop) or a game model (model), as in mesh_to_displacement. Targets: faces, ids or solid_ids, else the selected displacements or those of the selected solids. axis z, x, y or best (the normal of the displacements, default z); strength 0 to 1 blends from the current shape (default 1); offset adds units along the axis; origin, position, scale, up and convert_units place the mesh (origin default keep). Vertices outside the mesh stay as they are and are counted in missed. One undo step.",
        {
            "path": string("OBJ, FBX, glTF or GLB file: full path, or relative to the map folder."),
            "prop": integer("Id of a prop_static or prop_physics entity to use as the mesh.", 1, i64::from(u32::MAX)),
            "model": string("Game model path ending in .mdl, placed at the origin."),
            "up": choice("Up axis of a file (default file).", &["file", "y", "z"]),
            "convert_units": boolean("Meters of a file become Source units (default true)."),
            "faces": faces("Faces (default: the selected displacements)."), "ids": target_ids(), "solid_ids": target_solid_ids(),
            "axis": choice("Projection axis (default z).", &["z", "x", "y", "best"]),
            "flip": boolean("Project from the other side (default false)."),
            "pick": choice("Surface taken under the ray (default top).", &["top", "first"]),
            "scale": number("Scale of the mesh after the unit conversion (default 1).", 0.001, 1000.0),
            "origin": choice("Where the mesh goes (default keep).", &["center", "min_corner", "keep"]),
            "position": vec3("World position used by origin center and min_corner (default 0 0 0)."),
            "strength": number("Blend from the current shape (0) to the mesh (1, default).", 0.0, 1.0),
            "offset": number("Units added to the mesh height along the axis (default 0).", -16384.0, 16384.0),
        }, [];
    tile_displacements_from_selection: Edit, "Tile displacements from selection",
        "Cover the flat faces of the selected brushes with flat displacement tiles, so they can be shaped afterwards with conform_displacements_to_mesh or the displacement tools. Targets: ids or solid_ids, else the selected faces or brushes. Each face that is flat and square to axis (z, x, y or best; flip for the other side) is cut into equal tiles of at most tile_size units; power is auto (3), 2, 3 or 4; the source brushes are deleted unless delete_source is false. One undo step.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "axis": choice("Axis the tiled faces face (default z).", &["z", "x", "y", "best"]),
            "flip": boolean("Tile the faces that face the other way (default false)."),
            "tile_size": number("Largest tile width in units (default 256).", 16.0, 4096.0),
            "power": choice("Displacement power (default auto, which is 3).", &["auto", "2", "3", "4"]),
            "material": string("Texture of the tiles (default dev/dev_measuregeneric01b)."),
            "uv_scale": number("Texture scale (default 0.25).", 0.001, 64.0),
            "thickness": number("Thickness of the brushes under the displacements (default 16).", 1.0, 4096.0),
            "delete_source": boolean("Delete the tiled brushes (default true)."),
            "group": boolean("Group the new brushes (default true)."),
        }, [];
}

enum Src {
    Path(std::path::PathBuf),
    Ready(SourceMesh),
}

fn source_of(state: &AppState, a: &Args<'_>) -> R<Src> {
    let path = a.opt_str("path", 1024)?;
    let prop = a.opt_u64("prop", 1, u64::from(u32::MAX))?;
    let model = a.opt_str("model", 255)?;
    match (path, prop, model) {
        (Some(p), None, None) => Ok(Src::Path(source::resolve_path(state, p)?)),
        (None, Some(id), None) => source::from_prop(state, id as u32).map(Src::Ready),
        (None, None, Some(m)) => source::from_model(state, m).map(Src::Ready),
        (None, None, None) => Err("give the mesh: path (a file), prop (an entity id) or model (a .mdl)".to_owned()),
        _ => Err("give only one of path, prop and model".to_owned()),
    }
}

fn up_of(a: &Args<'_>) -> R<(UpChoice, bool)> {
    let up = match a.opt_str("up", 16)? {
        Some(s) => UpChoice::parse(s).ok_or_else(|| format!("up must be file, y or z, not {s}"))?,
        None => UpChoice::File,
    };
    Ok((up, a.opt_bool("convert_units")?.unwrap_or(true)))
}

fn axis_of(a: &Args<'_>) -> R<AxisChoice> {
    match a.opt_str("axis", 16)? {
        Some(s) => AxisChoice::parse(s).ok_or_else(|| format!("axis must be z, x, y or best, not {s}")),
        None => Ok(AxisChoice::Z),
    }
}

fn power_of(a: &Args<'_>) -> R<PowerChoice> {
    match a.opt_str("power", 16)? {
        Some(s) => PowerChoice::parse(s).ok_or_else(|| format!("power must be auto, 2, 3 or 4, not {s}")),
        None => Ok(PowerChoice::Auto),
    }
}

fn origin_of(a: &Args<'_>, default: Origin) -> R<Origin> {
    match a.opt_str("origin", 16)? {
        Some(s) => Origin::parse(s).ok_or_else(|| format!("origin must be center, min_corner or keep, not {s}")),
        None => Ok(default),
    }
}

fn pick_of(a: &Args<'_>) -> R<Pick> {
    match a.opt_str("pick", 16)? {
        Some(s) => Pick::parse(s).ok_or_else(|| format!("pick must be top or first, not {s}")),
        None => Ok(Pick::Top),
    }
}

fn material_of(a: &Args<'_>, default: &str) -> R<String> {
    match a.opt_str("material", 255)? {
        Some(m) => super::tools::material_name(m),
        None => Ok(default.to_owned()),
    }
}

fn mesh_options(a: &Args<'_>, source_is_file: bool) -> R<MeshDispOptions> {
    let d = MeshDispOptions::default();
    let fill = match a.opt_str("fill", 16)? {
        Some(s) => Fill::parse(s).ok_or_else(|| format!("fill must be nearest or base, not {s}"))?,
        None => d.fill,
    };
    Ok(MeshDispOptions {
        axis: axis_of(a)?,
        flip: a.opt_bool("flip")?.unwrap_or(false),
        tile_size: a.opt_f64("tile_size", 16.0, 4096.0)?.unwrap_or(d.tile_size),
        power: power_of(a)?,
        pick: pick_of(a)?,
        fill,
        margin: a.opt_u64("margin", 0, 8)?.unwrap_or(0) as u32,
        smooth: a.opt_u64("smooth", 0, 64)?.unwrap_or(0) as u32,
        scale: a.opt_f64("scale", 0.001, 1000.0)?.unwrap_or(1.0),
        origin: origin_of(a, if source_is_file { Origin::Center } else { Origin::Keep })?,
        position: a.opt_vec3("position")?.unwrap_or(DVec3::ZERO),
        material: material_of(a, &d.material)?,
        uv_scale: a.opt_f64("uv_scale", 0.001, 64.0)?.unwrap_or(d.uv_scale),
        skirt: a.opt_bool("skirt")?.unwrap_or(false),
        bottom: a.opt_bool("bottom")?.unwrap_or(false),
        base_height: a.opt_f64("base_height", -131_072.0, 131_072.0)?,
        thickness: a.opt_f64("thickness", 1.0, 4096.0)?.unwrap_or(d.thickness),
        group: a.opt_bool("group")?.unwrap_or(true),
    })
}

fn axis_name(plan: &TilePlan) -> &'static str {
    ["x", "y", "z"][plan.frame.axis()]
}

fn layout_json(plan: &TilePlan, opts: &MeshDispOptions, label: &str) -> Value {
    let w = plan.warnings;
    let mut warnings = Vec::new();
    if w.overhang_nodes > 0 {
        warnings.push(format!("{} vertices have an overhang: only the top surface is kept", w.overhang_nodes));
    }
    if w.over_limit {
        warnings.push(format!(
            "more than {} displacements: the engine accepts about that many in one map",
            mesh_disp::DISP_LIMIT
        ));
    }
    json!({
        "mesh": label,
        "axis": axis_name(plan),
        "flipped": plan.frame.flipped(),
        "power": plan.power,
        "tile_size": plan.tile,
        "vertex_spacing": plan.spacing,
        "tiles": plan.tile_count(),
        "skirt_tiles": if opts.skirt { plan.edge_count() } else { 0 },
        "skipped_tiles": plan.skipped,
        "plane": plan.plane,
        "base_height": plan.base,
        "filled_vertices": w.filled_nodes,
        "overhang_vertices": w.overhang_nodes,
        "warnings": warnings,
    })
}

fn created_json(plan: &TilePlan, opts: &MeshDispOptions, label: &str, r: &MeshDispResult) -> Value {
    let mut v = layout_json(plan, opts, label);
    let ids: Vec<u32> = r.solids.iter().copied().take(MAX_LISTED).collect();
    v["created"] = json!({
        "tiles": r.tiles,
        "skirts": r.skirts,
        "bottoms": r.bottoms,
        "solid_ids": ids,
        "truncated": r.solids.len() > MAX_LISTED,
        "group": r.group,
    });
    v
}

fn mesh_to_displacement(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let src = source_of(state, a)?;
    let (up, convert) = up_of(a)?;
    let is_file = matches!(&src, Src::Path(_)) || matches!(&src, Src::Ready(s) if s.is_file());
    let opts = mesh_options(a, is_file)?;
    mesh_disp::check_options(&opts).map_err(|e| e.to_string())?;
    let dry_run = a.opt_bool("dry_run")?.unwrap_or(false);
    if !dry_run && state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    Ok(Worker::spawn_then("mesh_to_displacement", move || {
        let fail = |e: String| (ToolResult::error(format!("mesh_to_displacement: {e}")), None);
        let source = match src {
            Src::Path(p) => match source::read_file(&p) {
                Ok(s) => s,
                Err(e) => return fail(e),
            },
            Src::Ready(s) => s,
        };
        let tris: Vec<Tri> = source.tris(up, convert);
        let plan = match mesh_disp::plan(&tris, &opts) {
            Ok(p) => p,
            Err(e) => return fail(e.to_string()),
        };
        let label = source.label.clone();
        if dry_run {
            return (ToolResult::json(layout_json(&plan, &opts, &label)), None);
        }
        let apply: ApplyFn = Box::new(move |state: &mut AppState| {
            let made = edit(state, "mesh_to_displacement", |doc| doc.create_mesh_displacements(&plan, &opts));
            Some(match made {
                Ok(r) => ToolResult::json(created_json(&plan, &opts, &label, &r)),
                Err(e) => ToolResult::error(format!("mesh_to_displacement: {e}")),
            })
        });
        (ToolResult::json(json!({})), Some(apply))
    }))
}

fn conform_targets(state: &AppState, a: &Args<'_>) -> R<Vec<FaceRef>> {
    let given = a.get("faces").is_some() || a.get("ids").is_some() || a.get("solid_ids").is_some();
    let is_disp = |f: &FaceRef| state.doc.face_side(*f).is_some_and(|s| s.disp.is_some());
    let mut faces: Vec<FaceRef> = if given {
        target_faces(state, a, "faces")?.into_iter().filter(is_disp).collect()
    } else {
        state.doc.selected_faces().into_iter().filter(is_disp).collect()
    };
    if faces.is_empty() && !given {
        let solids = state.doc.selected_solid_ids();
        faces = state.doc.disp_faces().into_iter().filter(|f| solids.contains(&f.solid)).collect();
    }
    if faces.is_empty() {
        return Err("no displacement among the targets (select some, or give faces, ids or solid_ids)".to_owned());
    }
    Ok(faces)
}

fn conform_displacements_to_mesh(state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let src = source_of(state, a)?;
    let (up, convert) = up_of(a)?;
    let faces = conform_targets(state, a)?;
    let opts = ConformOptions {
        axis: axis_of(a)?,
        flip: a.opt_bool("flip")?.unwrap_or(false),
        scale: a.opt_f64("scale", 0.001, 1000.0)?.unwrap_or(1.0),
        origin: origin_of(a, Origin::Keep)?,
        position: a.opt_vec3("position")?.unwrap_or(DVec3::ZERO),
        pick: pick_of(a)?,
        strength: a.opt_f64("strength", 0.0, 1.0)?.unwrap_or(1.0),
        offset: a.opt_f64("offset", -16384.0, 16384.0)?.unwrap_or(0.0),
    };
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    Ok(Worker::spawn_then("conform_displacements_to_mesh", move || {
        let fail = |e: String| (ToolResult::error(format!("conform_displacements_to_mesh: {e}")), None);
        let source = match src {
            Src::Path(p) => match source::read_file(&p) {
                Ok(s) => s,
                Err(e) => return fail(e),
            },
            Src::Ready(s) => s,
        };
        let tris: Vec<Tri> = source.tris(up, convert);
        let label = source.label.clone();
        let apply: ApplyFn = Box::new(move |state: &mut AppState| {
            let done =
                edit(state, "conform_displacements_to_mesh", |doc| doc.conform_displacements(&faces, &tris, &opts));
            Some(match done {
                Ok(r) => ToolResult::json(json!({
                    "mesh": label,
                    "displacements": r.faces,
                    "vertices_moved": r.vertices,
                    "missed": r.missed,
                    "overhang_vertices": r.overhang,
                })),
                Err(e) => ToolResult::error(format!("conform_displacements_to_mesh: {e}")),
            })
        });
        (ToolResult::json(json!({})), Some(apply))
    }))
}

fn tile_displacements_from_selection(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let d = TileSelectionOptions::default();
    let opts = TileSelectionOptions {
        axis: axis_of(a)?,
        flip: a.opt_bool("flip")?.unwrap_or(false),
        tile_size: a.opt_f64("tile_size", 16.0, 4096.0)?.unwrap_or(d.tile_size),
        power: power_of(a)?,
        material: material_of(a, &d.material)?,
        uv_scale: a.opt_f64("uv_scale", 0.001, 64.0)?.unwrap_or(d.uv_scale),
        thickness: a.opt_f64("thickness", 1.0, 4096.0)?.unwrap_or(d.thickness),
        delete_source: a.opt_bool("delete_source")?.unwrap_or(true),
        group: a.opt_bool("group")?.unwrap_or(true),
    };
    if state.doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit (a drag in a view?); try again".to_owned());
    }
    if a.get("ids").is_some() || a.get("solid_ids").is_some() {
        let objs = targets(state, a)?;
        state.doc.select(&objs, hp_editor::SelectOp::Replace);
    }
    let r = edit(state, "tile_displacements_from_selection", |doc| doc.tile_selection(&opts))?;
    let ids: Vec<u32> = r.solids.iter().copied().take(MAX_LISTED).collect();
    Ok(ToolResult::json(json!({
        "tiles": r.tiles,
        "source_brushes": r.sources,
        "deleted": r.removed,
        "skipped_faces": r.skipped_faces,
        "solid_ids": ids,
        "truncated": r.solids.len() > MAX_LISTED,
    })))
}

#[cfg(test)]
mod tests {
    use glam::DVec3;
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn write_pyramid(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("hp_mesh_disp_mcp_test");
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join(name);
        let obj = "v 0 0 0\nv 256 0 0\nv 256 256 0\nv 0 256 0\nv 128 128 100\n\
                   f 1 5 2\nf 2 5 3\nf 3 5 4\nf 4 5 1\nf 1 2 3\nf 1 3 4\n";
        std::fs::write(&path, obj).expect("obj");
        path
    }

    #[test]
    fn a_file_becomes_tiles_and_back_with_undo() {
        let mut s = hp_app::testing::test_state();
        let path = write_pyramid("pyramid.obj");
        let p = path.display().to_string();
        let before = s.doc.disp_faces().len();
        let r = ok(
            &mut s,
            "mesh_to_displacement",
            json!({ "path": p, "dry_run": true, "convert_units": false, "up": "z", "origin": "keep" }),
        );
        assert_eq!((r["tiles"].as_u64(), r["axis"].as_str()), (Some(1), Some("z")));
        assert_eq!(s.doc.disp_faces().len(), before);
        let steps = s.doc.history_len().0;
        let r = ok(
            &mut s,
            "mesh_to_displacement",
            json!({ "path": p, "convert_units": false, "up": "z", "origin": "keep", "power": "3", "skirt": true, "bottom": true }),
        );
        assert_eq!(r["created"]["tiles"], 1);
        let centered = ok(
            &mut s,
            "mesh_to_displacement",
            json!({ "path": p, "dry_run": true, "convert_units": false, "up": "z" }),
        );
        assert_eq!(centered["tiles"], 4, "a centered mesh straddles the origin");
        assert_eq!(r["created"]["skirts"], 4);
        assert_eq!(r["power"], 3);
        assert_eq!(s.doc.history_len().0, steps + 1);
        assert_eq!(s.doc.disp_faces().len(), before + 5);
        assert!(s.doc.undo());
        assert_eq!(s.doc.disp_faces().len(), before);
        err(&mut s, "mesh_to_displacement", json!({}));
        err(&mut s, "mesh_to_displacement", json!({ "path": p, "model": "models/a.mdl" }));
        err(&mut s, "mesh_to_displacement", json!({ "path": "../x.obj" }));
        err(&mut s, "mesh_to_displacement", json!({ "path": "C:/definitely/missing.obj" }));
        err(&mut s, "mesh_to_displacement", json!({ "path": p, "tile_size": 5 }));
        err(&mut s, "mesh_to_displacement", json!({ "path": p, "base_height": 50, "convert_units": false, "up": "z" }));
        err(&mut s, "mesh_to_displacement", json!({ "model": "models/missing.obj" }));
    }

    #[test]
    fn displacements_conform_to_a_file() {
        let mut s = hp_app::testing::test_state();
        let path = write_pyramid("pyramid2.obj");
        let p = path.display().to_string();
        err(&mut s, "conform_displacements_to_mesh", json!({ "path": p }));
        ok(
            &mut s,
            "mesh_to_displacement",
            json!({ "path": p, "convert_units": false, "up": "z", "origin": "keep", "power": "2", "tile_size": 256 }),
        );
        let faces = s.doc.disp_faces();
        let face = faces.last().copied().expect("tile");
        s.doc.select_faces(&[face], hp_editor::SelectOp::Replace);
        let r = ok(
            &mut s,
            "conform_displacements_to_mesh",
            json!({ "path": p, "convert_units": false, "up": "z", "strength": 0.5, "offset": 4 }),
        );
        assert_eq!(r["displacements"], 1);
        assert_eq!(r["missed"], 0);
        let mesh = s.doc.scene().solid(face.solid).and_then(|n| n.disp_mesh(face.side)).expect("mesh");
        let apex = mesh.positions.iter().map(|q: &DVec3| q.z).fold(f64::MIN, f64::max);
        assert!((apex - 102.0).abs() < 1e-6, "{apex}");
        assert!(s.doc.undo());
        err(&mut s, "conform_displacements_to_mesh", json!({ "path": p, "strength": 3 }));
    }

    #[test]
    fn selected_brushes_are_tiled() {
        let mut s = hp_app::testing::test_state();
        let r = ok(
            &mut s,
            "tile_displacements_from_selection",
            json!({ "solid_ids": [3], "tile_size": 128, "axis": "best" }),
        );
        assert!(r["tiles"].as_u64().unwrap_or(0) >= 1, "{r}");
        err(&mut s, "tile_displacements_from_selection", json!({ "tile_size": 1 }));
    }
}
