use hp_app::AppState;
use hp_editor::display_color::{DisplayColor, DisplayColorState};
use hp_editor::{FaceRef, RectMode, SelectOp};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, R, bounds_json, edit, obj_json, v3};

const MAX_SETS: usize = 4096;

domain! { "groups and visibility";
    select_by_class: Editor, "Select by class",
        "Select entities whose class matches a pattern (* and ?, case insensitive).",
        { "pattern": string("Class pattern, for example \"prop_*\"."), "op": op() }, ["pattern"];
    select_by_targetname: Editor, "Select by name",
        "Select entities whose targetname matches a pattern (* and ?).",
        { "pattern": string("Targetname pattern."), "op": op() }, ["pattern"];
    select_by_material: Editor, "Select by material",
        "Select the solids (or with faces: true, the faces) using a material pattern (* and ?).",
        { "pattern": string("Material pattern, for example \"tools/*\"."), "faces": boolean("Select faces instead of solids."), "op": op() }, ["pattern"];
    select_in_box: Editor, "Select in a box",
        "Select the visible objects touching (or inside, or with their center inside) a box.",
        {
            "min": vec3("Box minimum."), "max": vec3("Box maximum."),
            "mode": choice("Which objects (default touching).", &["touching", "inside", "center"]),
            "op": op(),
        }, ["min", "max"];
    select_faces: Editor, "Select faces",
        "Select faces (texture application mode): [{solid, side}].",
        { "faces": faces("Faces."), "op": op() }, ["faces"];
    select_elements: Editor, "Select edges or vertices",
        "Select edges or vertices of one brush, the same selection the selection tool makes with its element option \
        set to edge or vertex (keys 2 and 1). indices are edge numbers (get_brush_edges) or vertex numbers \
        (get_brush_vertices) of the solid. The brush itself is selected too. The bevel tool (Ctrl+B) bevels the \
        selected edges and the vertex tool opens with them selected. Faces are selected with select_faces. \
        get_selection answers the selected edges and vertices.",
        {
            "element": choice("What the indices name.", &["edge", "vertex"]),
            "solid": integer("Solid id.", 1, i64::from(u32::MAX)),
            "indices": { "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 65535 }, "minItems": 1, "maxItems": 512, "description": "Edge or vertex numbers of the solid." },
            "op": op(),
        }, ["element", "solid", "indices"];
    select_all: Editor, "Select all",
        "Select every visible object (or invert the selection with invert: true).",
        { "invert": boolean("Invert the selection instead.") }, [];
    select_group: Editor, "Select a group or visgroup",
        "Select the members of a group (group) or of a visgroup (visgroup).",
        {
            "group": integer("Group id.", 1, i64::from(u32::MAX)),
            "visgroup": integer("Visgroup id.", 1, i64::from(u32::MAX)),
            "op": op(),
        }, [];
    frame_objects: Editor, "Frame objects",
        "Point the 3D camera at objects (default: the selection) so they fill the view, and center the 2D views on \
        them.",
        { "ids": target_ids(), "solid_ids": target_solid_ids(), "views": choice("Which views (default all).", &["all", "3d", "2d"]) }, [];
    list_groups: Read, "List groups",
        "The groups of the map: id, parent group, members and bounds.",
        { "limit": limit(), "offset": offset() }, [];
    group_objects: Edit, "Group objects",
        "Group objects (Ctrl+G); groups fully inside are nested. Returns the group id.",
        { "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    ungroup_objects: Edit, "Ungroup objects",
        "Dissolve the groups of objects (Ctrl+U).",
        { "ids": target_ids(), "solid_ids": target_solid_ids(), "groups": ids("Group ids to dissolve.") }, [];
    list_visgroups: Read, "List visgroups",
        "The visgroup tree: id, name, color, parent, visible (true, false or mixed) and member counts.",
        {}, [];
    create_visgroup: Edit, "Create a visgroup",
        "Create a visgroup (under a parent), optionally with members and a color.",
        {
            "name": string("Name."),
            "parent": integer("Parent visgroup id.", 1, i64::from(u32::MAX)),
            "ids": ids("Members (entity or solid ids)."), "solid_ids": target_solid_ids(),
            "color": color(),
        }, ["name"];
    edit_visgroup: Edit, "Edit a visgroup",
        "Rename a visgroup, change its color, move it under another parent (0: top level) or up and down.",
        {
            "id": integer("Visgroup id.", 1, i64::from(u32::MAX)),
            "name": string("New name."),
            "color": color(),
            "parent": integer("New parent visgroup, 0 for the top level.", 0, i64::from(u32::MAX)),
            "move": integer("Move up (negative) or down among its siblings.", -1000, 1000),
        }, ["id"];
    delete_visgroup: Destructive, "Delete a visgroup",
        "Delete a visgroup (its objects stay, they only leave it).",
        { "id": integer("Visgroup id.", 1, i64::from(u32::MAX)) }, ["id"];
    set_visgroup_members: Edit, "Visgroup members",
        "Add objects to a visgroup, remove them from it, or set exactly the visgroups of objects.",
        {
            "action": choice("add, remove, or set (the objects get exactly visgroups).", &["add", "remove", "set"]),
            "visgroup": integer("Visgroup (add and remove).", 1, i64::from(u32::MAX)),
            "visgroups": ids("Visgroups (set)."),
            "ids": target_ids(), "solid_ids": target_solid_ids(),
        }, ["action"];
    move_to_visgroup: Edit, "Move to visgroup",
        "Put objects in a visgroup (existing, or new with new_name), optionally leaving their other visgroups and \
        hiding the visgroup.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "visgroup": integer("Existing visgroup.", 1, i64::from(u32::MAX)),
            "new_name": string("Name of a new visgroup (without visgroup)."),
            "remove_others": boolean("Leave the other visgroups (default true)."),
            "hide": boolean("Hide the visgroup afterwards (default false)."),
        }, [];
    set_visgroup_visible: Edit, "Show or hide a visgroup",
        "Show or hide a visgroup (with its children), or every visgroup at once (all: true, with visible: false to hide them all).",
        {
            "id": integer("Visgroup id.", 1, i64::from(u32::MAX)),
            "visible": boolean("Visible."),
            "all": boolean("Every visgroup at once (visible defaults to true)."),
        }, [];
    hidden_to_visgroup: Edit, "Hidden objects to a visgroup",
        "Move every hidden object into a new visgroup (Hammer++).",
        {}, [];
    hide_objects: Edit, "Hide objects",
        "Hide objects (hide: true, default) or show them again; unselected: true hides everything not selected; \
        unhide_all: true shows every hidden object.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "hide": boolean("Hide (true) or show (false)."),
            "unselected": boolean("Hide the objects that are not selected."),
            "unhide_all": boolean("Show every hidden object."),
        }, [];
    isolate_objects: Edit, "Isolate objects",
        "Hide everything but some objects (default: the selection), to look at them alone; hide_objects with \
        unhide_all shows everything again.",
        { "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    get_visibility: Read, "Visibility of objects",
        "Whether objects are visible, hidden, in a hidden visgroup or cordoned out, and the counts of hidden objects.",
        { "ids": ids("Objects to look at (default: none, counts only)."), "solid_ids": target_solid_ids() }, [];
    list_cordons: Read, "List cordons",
        "The cordons (named boxes that limit what is shown and compiled), whether cordoning is on.",
        {}, [];
    add_cordon: Edit, "Add a cordon",
        "Add a cordon box (active by default) and switch cordoning on (enable false keeps it as it is).",
        {
            "name": string("Name (default \"cordon\")."),
            "min": vec3("Box minimum."), "max": vec3("Box maximum."),
            "active": boolean("This cordon is active (default true)."),
            "enable": boolean("Switch cordoning on (default true)."),
        }, ["min", "max"];
    edit_cordon: Edit, "Edit a cordon",
        "Change a cordon by index (list_cordons): name, box, active; or delete it (delete: true).",
        {
            "index": integer("Cordon index.", 0, 4095),
            "name": string("New name."),
            "min": vec3("New box minimum (with max)."), "max": vec3("New box maximum (with min)."),
            "active": boolean("Active."),
            "delete": boolean("Delete the cordon."),
        }, ["index"];
    set_cordons_enabled: Edit, "Cordoning on or off",
        "Switch cordoning on or off for the whole map (views and compiles).",
        { "enabled": boolean("On.") }, ["enabled"];
    get_display_color: Read, "Display colors",
        "The display color of objects (default: the selection) in the 2D views: color [r, g, b] when one was \
        chosen, null for the default look (white world brushes, entities in their game data color), the common \
        state (none, default, custom or mixed) and the render.object_colors_2d and render.object_colors_3d \
        settings.",
        { "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    set_display_color: Edit, "Set the display color",
        "Give objects (default: the selection; a selected group colors every member) a display color for the 2D \
        views: color [r, g, b], random: true (each brush and entity its own color) or reset: true (default look). \
        Saved in the map as the editor color, which Hammer and Hammer++ show too. One undo step. Toggle the colors \
        with set_setting render.object_colors_2d and render.object_colors_3d; edit.new_brush_color ([r, g, b] or \
        null) colors new brushes.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "color": color(),
            "random": boolean("A different color for each object."),
            "reset": boolean("Back to the default look."),
        }, [];
}

fn get_display_color(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let doc = &state.doc;
    let list: Vec<Value> = objs
        .iter()
        .take(MAX_SETS)
        .map(|o| {
            let mut v = obj_json(*o);
            v["color"] = json!(doc.display_color_of(*o).flatten());
            v
        })
        .collect();
    let r = &state.settings.render;
    Ok(ToolResult::json(json!({
        "state": display_state_json(doc.display_color_state(&objs)),
        "objects": list,
        "object_colors_2d": r.object_colors_2d,
        "object_colors_3d": r.object_colors_3d,
    })))
}

fn display_state_json(s: DisplayColorState) -> Value {
    match s {
        DisplayColorState::Empty => json!("none"),
        DisplayColorState::Default => json!("default"),
        DisplayColorState::Custom(c) => json!({ "custom": c }),
        DisplayColorState::Mixed => json!("mixed"),
    }
}

fn set_display_color(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let color = opt_color(a)?;
    let random = a.opt_bool("random")?.unwrap_or(false);
    let reset = a.opt_bool("reset")?.unwrap_or(false);
    let change = match (color, random, reset) {
        (Some(c), false, false) => DisplayColor::Set(c),
        (None, true, false) => DisplayColor::Random,
        (None, false, true) => DisplayColor::Reset,
        _ => return Err("give exactly one of color, random: true or reset: true".to_owned()),
    };
    let changed = edit(state, "set_display_color", |doc| doc.set_display_color(&objs, change, false))?;
    Ok(ToolResult::json(json!({
        "changed": changed,
        "objects": objs.len(),
        "state": display_state_json(state.doc.display_color_state(&objs)),
    })))
}

fn op() -> Value {
    choice("How the selection changes (default replace).", &["replace", "add", "remove", "toggle"])
}

fn color() -> Value {
    json!({ "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 255 }, "minItems": 3, "maxItems": 3, "description": "Color [r, g, b] 0 to 255." })
}

fn sel_op(a: &Args<'_>) -> R<SelectOp> {
    Ok(match a.opt_enum("op", &["replace", "add", "remove", "toggle"])?.as_deref() {
        Some("add") => SelectOp::Add,
        Some("remove") => SelectOp::Remove,
        Some("toggle") => SelectOp::Toggle,
        _ => SelectOp::Replace,
    })
}

fn opt_color(a: &Args<'_>) -> R<Option<[u8; 3]>> {
    let Some(v) = a.get("color") else { return Ok(None) };
    let bad = || "color must be [r, g, b] with integers from 0 to 255".to_owned();
    let arr = v.as_array().filter(|x| x.len() == 3).ok_or_else(bad)?;
    let mut out = [0u8; 3];
    for (o, x) in out.iter_mut().zip(arr) {
        *o = x.as_u64().filter(|n| *n <= 255).ok_or_else(bad)? as u8;
    }
    Ok(Some(out))
}

fn pattern(a: &Args<'_>) -> R<String> {
    let p = a.str("pattern", MAX_NAME)?.trim();
    if p.is_empty() {
        return Err("pattern must not be empty".to_owned());
    }
    Ok(p.to_owned())
}

pub(super) fn element_json(state: &AppState) -> Value {
    let revision = state.doc.revision();
    let sel = &state.runtime.selection;
    let edges: Vec<Value> = sel
        .fresh_edges(revision)
        .iter()
        .take(200)
        .map(|e| json!({ "solid": e.solid, "a": v3(e.a), "b": v3(e.b) }))
        .collect();
    let vertices: Vec<Value> =
        sel.fresh_vertices(revision).iter().take(200).map(|(s, p)| json!({ "solid": s, "point": v3(*p) })).collect();
    json!({
        "element": state.settings.selection.element.id(),
        "edges": edges,
        "edge_count": sel.fresh_edges(revision).len(),
        "vertices": vertices,
        "vertex_count": sel.fresh_vertices(revision).len(),
    })
}

fn selection_json(state: &AppState) -> Value {
    let objs = state.doc.selected_objects();
    let faces = state.doc.selected_faces();
    let mut out = json!({
        "selected": objs.len(),
        "faces": faces.len(),
        "objects": objs.iter().take(200).map(|o| obj_json(*o)).collect::<Vec<_>>(),
        "bounds": bounds_json(state.doc.selection_bounds(&bounds_fn(state))),
    });
    if let (Some(map), Value::Object(extra)) = (out.as_object_mut(), element_json(state)) {
        map.extend(extra);
    }
    out
}

fn visgroup_exists(state: &AppState, id: u32) -> R<()> {
    state.doc.visgroup(id).map(|_| ()).ok_or_else(|| format!("no visgroup has the id {id}"))
}

fn name_arg(a: &Args<'_>, key: &str) -> R<Option<String>> {
    match a.opt_str(key, 128)? {
        None => Ok(None),
        Some(n) => {
            let n = n.trim();
            if n.is_empty() || n.contains(['"', '\n', '\r', '\0']) {
                return Err(format!("{key} must be a name without quotes or line breaks"));
            }
            Ok(Some(n.to_owned()))
        }
    }
}

fn select_by_class(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let n = state.doc.select_by_class(&pattern(a)?, sel_op(a)?);
    Ok(ToolResult::json(json!({ "matched": n, "selection": selection_json(state) })))
}

fn select_by_targetname(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let n = state.doc.select_by_targetname(&pattern(a)?, sel_op(a)?);
    Ok(ToolResult::json(json!({ "matched": n, "selection": selection_json(state) })))
}

fn select_by_material(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let p = pattern(a)?.replace('\\', "/");
    let op = sel_op(a)?;
    let n = if a.opt_bool("faces")?.unwrap_or(false) {
        state.doc.select_faces_by_material(&p, op)
    } else {
        state.doc.select_by_material(&p, op)
    };
    Ok(ToolResult::json(json!({ "matched": n, "selection": selection_json(state) })))
}

fn select_in_box(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let b = hp_geom::Aabb::new(a.vec3("min")?, a.vec3("max")?);
    let mode = match a.opt_enum("mode", &["touching", "inside", "center"])?.as_deref() {
        Some("inside") => RectMode::Inside,
        Some("center") => RectMode::Center,
        _ => RectMode::Touching,
    };
    let op = sel_op(a)?;
    let AppState { doc, assets, .. } = state;
    let f = |e: &hp_vmf::Entity| assets.entity_bounds(e);
    let n = doc.select_in_box(&b, mode, &f, op);
    Ok(ToolResult::json(json!({ "matched": n, "selection": selection_json(state) })))
}

fn select_faces(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces: Vec<FaceRef> = opt_faces(&state.doc, a, "faces")?.unwrap_or_default();
    state.doc.select_faces(&faces, sel_op(a)?);
    Ok(ToolResult::json(selection_json(state)))
}

fn select_elements(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let element = a.opt_enum("element", &["edge", "vertex"])?.ok_or("element is required")?;
    let solid = solid_id(&state.doc, a, "solid")?;
    let list = a
        .get("indices")
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty() && l.len() <= 512)
        .ok_or("indices must be a list of 1 to 512 numbers")?;
    let mut indices = Vec::with_capacity(list.len());
    for v in list {
        indices.push(v.as_u64().map(|i| i as usize).ok_or("indices must be non-negative integers")?);
    }
    let op = sel_op(a)?;
    let node = state.doc.scene().solid(solid).filter(|n| n.geom.valid).ok_or("the solid has no valid geometry")?;
    let (edges, verts) = if element == "edge" {
        let all = node.geom.edges();
        let mut out = Vec::with_capacity(indices.len());
        for i in &indices {
            let [p, q] = all.get(*i).copied().ok_or_else(|| format!("the solid has {} edges", all.len()))?;
            out.push(hp_editor::EdgeRef::new(solid, p, q));
        }
        (out, Vec::new())
    } else {
        let (points, _) = state.doc.solid_vertices(solid).ok_or("the solid has no valid geometry")?;
        let mut out = Vec::with_capacity(indices.len());
        for i in &indices {
            let p = points.get(*i).copied().ok_or_else(|| format!("the solid has {} vertices", points.len()))?;
            out.push((solid, p));
        }
        (Vec::new(), out)
    };
    let want =
        if element == "edge" { hp_app::settings::SelectElement::Edge } else { hp_app::settings::SelectElement::Vertex };
    if state.settings.selection.element != want {
        state.set_pick_element(want);
    }
    state.doc.select(
        &[hp_editor::ObjRef::Solid(solid)],
        if op == SelectOp::Replace { SelectOp::Replace } else { SelectOp::Add },
    );
    let revision = state.doc.revision();
    let sel = &mut state.runtime.selection;
    if sel.element_revision != revision || op == SelectOp::Replace {
        sel.clear_elements();
    }
    sel.element_revision = revision;
    if element == "edge" {
        sel.vertices.clear();
    } else {
        sel.edges.clear();
    }
    for e in edges {
        let at = sel.edges.iter().position(|x| x.same_edge(&e));
        match (op, at) {
            (SelectOp::Remove, Some(i)) => {
                sel.edges.remove(i);
            }
            (SelectOp::Toggle, Some(i)) => {
                sel.edges.remove(i);
            }
            (SelectOp::Remove, None) | (_, Some(_)) => {}
            (_, None) => sel.edges.push(e),
        }
    }
    for (id, p) in verts {
        let at = sel.vertices.iter().position(|(i, q)| *i == id && q.distance(p) < 1e-3);
        match (op, at) {
            (SelectOp::Remove, Some(i)) => {
                sel.vertices.remove(i);
            }
            (SelectOp::Toggle, Some(i)) => {
                sel.vertices.remove(i);
            }
            (SelectOp::Remove, None) | (_, Some(_)) => {}
            (_, None) => sel.vertices.push((id, p)),
        }
    }
    state.commit_picks();
    state.settings_dirty = true;
    Ok(ToolResult::json(selection_json(state)))
}

fn select_all(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("invert")?.unwrap_or(false) {
        state.doc.invert_selection();
    } else {
        state.doc.select_all();
    }
    Ok(ToolResult::json(selection_json(state)))
}

fn select_group(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let op = sel_op(a)?;
    let members = match (a.opt_u64("group", 1, u64::from(u32::MAX))?, a.opt_u64("visgroup", 1, u64::from(u32::MAX))?) {
        (Some(g), None) => {
            let g = g as u32;
            if state.doc.scene().group(g).is_none() {
                return Err(format!("no group has the id {g}"));
            }
            state.doc.scene().members_of_groups(&state.doc.scene().group_subtree(g))
        }
        (None, Some(v)) => {
            let v = v as u32;
            visgroup_exists(state, v)?;
            state.doc.visgroup_members(v)
        }
        _ => return Err("give group or visgroup".to_owned()),
    };
    state.doc.select(&members, op);
    Ok(ToolResult::json(json!({ "members": members.len(), "selection": selection_json(state) })))
}

fn frame_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let b = {
        let f = bounds_fn(state);
        state.doc.scene().objects_bounds(&objs, &f).filter(|b| !b.is_empty()).ok_or("the objects have no bounds")?
    };
    let views = a.opt_enum("views", &["all", "3d", "2d"])?.unwrap_or_else(|| "all".to_owned());
    if views != "2d" {
        state.cameras.cam3d.frame_box(b.min, b.max);
    }
    if views != "3d" {
        state.cameras.frame_2d(b.min, b.max, egui::vec2(800.0, 600.0));
    }
    let c = &state.cameras.cam3d;
    Ok(ToolResult::json(json!({
        "bounds": bounds_json(Some(b)),
        "camera_3d": { "position": super::tools::v3(c.pos), "yaw": c.yaw_deg, "pitch": c.pitch_deg },
        "zoom_2d": state.cameras.top.zoom,
    })))
}

fn list_groups(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let scene = state.doc.scene();
    let mut ids: Vec<u32> = scene.groups().keys().copied().collect();
    ids.sort_unstable();
    let (ids, total) = page(ids, a)?;
    let f = bounds_fn(state);
    let list: Vec<Value> = ids
        .into_iter()
        .map(|g| {
            let members = scene.group_members(g);
            json!({
                "id": g,
                "parent": scene.group_parent(g),
                "members": members.iter().take(100).map(|o| obj_json(*o)).collect::<Vec<_>>(),
                "member_count": members.len(),
                "bounds": bounds_json(scene.objects_bounds(&scene.members_of_groups(&scene.group_subtree(g)), &f)),
            })
        })
        .collect();
    Ok(ToolResult::json(json!({ "total": total, "groups": list })))
}

fn group_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let id = edit(state, "group_objects", |doc| doc.group(&objs))?;
    Ok(ToolResult::json(json!({ "group": id, "members": objs.len() })))
}

fn ungroup_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let mut objs = Vec::new();
    if a.get("groups").is_some() {
        for g in a.ids("groups")? {
            if state.doc.scene().group(g).is_none() {
                return Err(format!("no group has the id {g}"));
            }
            objs.extend(state.doc.scene().group_members(g));
        }
        if a.get("ids").is_some() || a.get("solid_ids").is_some() {
            objs.extend(targets(state, a)?);
        }
    } else {
        objs = targets(state, a)?;
    }
    if objs.is_empty() {
        return Err("the groups are empty".to_owned());
    }
    let n = edit(state, "ungroup_objects", |doc| doc.ungroup(&objs))?;
    Ok(ToolResult::json(json!({ "groups_removed": n })))
}

fn list_visgroups(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    let counts = state.doc.visgroup_member_counts();
    fn walk(
        state: &AppState,
        counts: &hp_editor::IdMap<usize>,
        list: &[hp_vmf::VisGroup],
        parent: Option<u32>,
        out: &mut Vec<Value>,
    ) {
        for v in list {
            if out.len() >= MAX_SETS {
                return;
            }
            let visible = match state.doc.visgroup_state(v.id) {
                hp_editor::VisgroupState::Shown => json!(true),
                hp_editor::VisgroupState::Hidden => json!(false),
                _ => json!("mixed"),
            };
            out.push(json!({
                "id": v.id,
                "name": v.name,
                "color": v.color,
                "parent": parent,
                "visible": visible,
                "members": counts.get(&v.id).copied().unwrap_or(0),
            }));
            walk(state, counts, &v.children, Some(v.id), out);
        }
    }
    let mut out = Vec::new();
    walk(state, &counts, state.doc.scene().visgroups(), None, &mut out);
    Ok(ToolResult::json(json!({ "visgroups": out })))
}

fn create_visgroup(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let name = name_arg(a, "name")?.ok_or("name is required")?;
    let parent = a.opt_u64("parent", 1, u64::from(u32::MAX))?.map(|p| p as u32);
    if let Some(p) = parent {
        visgroup_exists(state, p)?;
    }
    let members = super::tools::objects(&state.doc, a)?;
    let color = opt_color(a)?;
    let id = edit(state, "create_visgroup", |doc| {
        let id = doc.create_visgroup(&name, parent, &members)?;
        if let Some(c) = color {
            doc.set_visgroup_color(id, c)?;
        }
        Ok(id)
    })?;
    Ok(ToolResult::json(json!({ "id": id, "name": name, "members": members.len() })))
}

fn edit_visgroup(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = a.id("id")?;
    visgroup_exists(state, id)?;
    let name = name_arg(a, "name")?;
    let color = opt_color(a)?;
    let parent = a.opt_u64("parent", 0, u64::from(u32::MAX))?.map(|p| p as u32);
    if let Some(p) = parent.filter(|p| *p != 0) {
        visgroup_exists(state, p)?;
    }
    let delta = opt_i64(a, "move", -1000, 1000)?.map(|d| d as i32);
    if name.is_none() && color.is_none() && parent.is_none() && delta.is_none() {
        return Err("give name, color, parent or move".to_owned());
    }
    edit(state, "edit_visgroup", |doc| {
        if let Some(n) = &name {
            doc.rename_visgroup(id, n)?;
        }
        if let Some(c) = color {
            doc.set_visgroup_color(id, c)?;
        }
        if let Some(p) = parent {
            doc.move_visgroup(id, (p != 0).then_some(p))?;
        }
        if let Some(d) = delta.filter(|d| *d != 0) {
            if !doc.can_reorder_visgroup(id, d) {
                return Err(hp_editor::EditError::Invalid("the visgroup cannot move that far".to_owned()));
            }
            doc.reorder_visgroup(id, d)?;
        }
        Ok(())
    })?;
    Ok(ToolResult::json(json!({ "id": id })))
}

fn delete_visgroup(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = a.id("id")?;
    visgroup_exists(state, id)?;
    edit(state, "delete_visgroup", |doc| doc.delete_visgroup(id))?;
    Ok(ToolResult::json(json!({ "deleted": id })))
}

fn set_visgroup_members(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let action = a.opt_enum("action", &["add", "remove", "set"])?.ok_or("action is required")?;
    let objs = targets(state, a)?;
    let n = match action.as_str() {
        "set" => {
            if a.get("visgroup").is_some() {
                return Err("set takes visgroups (a list), not visgroup".to_owned());
            }
            let list = a.ids("visgroups")?;
            for v in &list {
                visgroup_exists(state, *v)?;
            }
            edit(state, "set_visgroup_members", |doc| doc.set_visgroups(&objs, &list))?
        }
        _ => {
            let v = a.opt_u64("visgroup", 1, u64::from(u32::MAX))?.ok_or("visgroup is required")? as u32;
            visgroup_exists(state, v)?;
            if action == "add" {
                edit(state, "set_visgroup_members", |doc| doc.add_to_visgroup(v, &objs))?
            } else {
                edit(state, "set_visgroup_members", |doc| doc.remove_from_visgroup(v, &objs))?
            }
        }
    };
    Ok(ToolResult::json(json!({ "changed": n })))
}

fn move_to_visgroup(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let target = a.opt_u64("visgroup", 1, u64::from(u32::MAX))?.map(|v| v as u32);
    let new_name = name_arg(a, "new_name")?;
    match (target, &new_name) {
        (Some(v), None) => visgroup_exists(state, v)?,
        (None, Some(_)) => {}
        _ => return Err("give visgroup or new_name".to_owned()),
    }
    let remove_others = a.opt_bool("remove_others")?.unwrap_or(true);
    let hide = a.opt_bool("hide")?.unwrap_or(false);
    let name = new_name.unwrap_or_default();
    let id = edit(state, "move_to_visgroup", |doc| doc.move_to_visgroup(&objs, target, &name, remove_others, hide))?;
    Ok(ToolResult::json(json!({ "visgroup": id, "objects": objs.len() })))
}

fn set_visgroup_visible(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("all")?.unwrap_or(false) {
        if a.get("id").is_some() {
            return Err("all cannot be combined with id".to_owned());
        }
        let visible = a.opt_bool("visible")?.unwrap_or(true);
        edit(state, "set_visgroup_visible", |doc| {
            if visible {
                doc.show_all_visgroups();
            } else {
                doc.hide_all_visgroups();
            }
            Ok(())
        })?;
        return Ok(ToolResult::json(json!({ "all_visible": visible })));
    }
    let id = a.opt_u64("id", 1, u64::from(u32::MAX))?.ok_or("give id (and visible), or all")? as u32;
    visgroup_exists(state, id)?;
    let visible = a.opt_bool("visible")?.ok_or("visible is required")?;
    edit(state, "set_visgroup_visible", |doc| doc.set_visgroup_visible(id, visible))?;
    Ok(ToolResult::json(json!({ "id": id, "visible": visible })))
}

fn hidden_to_visgroup(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    match edit(state, "hidden_to_visgroup", |doc| doc.hidden_to_visgroup())? {
        Some((id, n)) => Ok(ToolResult::json(json!({ "visgroup": id, "objects": n }))),
        None => Err("no object is hidden".to_owned()),
    }
}

fn hide_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let unselected = a.opt_bool("unselected")?.unwrap_or(false);
    let unhide_all = a.opt_bool("unhide_all")?.unwrap_or(false);
    let given = a.get("ids").is_some() || a.get("solid_ids").is_some() || a.get("hide").is_some();
    let n = match (unselected, unhide_all) {
        (true, true) => return Err("unselected and unhide_all cannot be combined".to_owned()),
        (true, false) | (false, true) if given => {
            return Err("unselected and unhide_all take no ids or hide".to_owned());
        }
        (true, false) => edit(state, "hide_objects", |doc| Ok(doc.hide_unselected()))?,
        (false, true) => edit(state, "hide_objects", |doc| Ok(doc.unhide_all()))?,
        (false, false) => {
            let objs = targets(state, a)?;
            let hide = a.opt_bool("hide")?.unwrap_or(true);
            edit(state, "hide_objects", |doc| Ok(doc.set_hidden(&objs, hide)))?
        }
    };
    Ok(ToolResult::json(json!({ "changed": n })))
}

fn isolate_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let keep: std::collections::HashSet<hp_editor::ObjRef> = objs.iter().copied().collect();
    let scene = state.doc.scene();
    let others: Vec<hp_editor::ObjRef> = scene
        .entities()
        .keys()
        .map(|id| hp_editor::ObjRef::Entity(*id))
        .chain(scene.solids().keys().map(|id| scene.object_of_solid(*id)))
        .filter(|o| !keep.contains(o))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter(|o| match o {
            hp_editor::ObjRef::Solid(id) => !keep.contains(&hp_editor::ObjRef::Solid(*id)),
            hp_editor::ObjRef::Entity(_) => true,
        })
        .collect();
    let n = edit(state, "isolate_objects", |doc| Ok(doc.set_hidden(&others, true)))?;
    Ok(ToolResult::json(json!({ "hidden": n, "kept": objs.len() })))
}

fn get_visibility(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = super::tools::objects(&state.doc, a)?;
    let doc = &state.doc;
    let scene = doc.scene();
    let hidden_solids = scene.solids().keys().filter(|id| !doc.is_solid_visible(**id)).count();
    let hidden_entities = scene.entities().keys().filter(|id| !doc.is_entity_visible(**id)).count();
    let list: Vec<Value> = objs
        .iter()
        .map(|o| {
            let info = scene.editor_info(*o);
            json!({
                "object": obj_json(*o),
                "visible": doc.is_visible(*o),
                "hidden_flag": info.is_some_and(|e| !e.visgroup_shown),
                "visgroups": info.map(|e| e.visgroup_ids.clone()).unwrap_or_default(),
                "cordoned_out": doc.is_cordoned_out(*o),
            })
        })
        .collect();
    Ok(ToolResult::json(json!({
        "hidden_solids": hidden_solids,
        "hidden_entities": hidden_entities,
        "objects": list,
    })))
}

fn cordons_json(state: &AppState) -> Value {
    let c = state.doc.scene().cordons();
    json!({
        "enabled": c.active,
        "cordons": c.cordons.iter().enumerate().map(|(i, k)| json!({
            "index": i,
            "name": k.name,
            "active": k.active,
            "min": super::tools::v3(k.mins),
            "max": super::tools::v3(k.maxs),
        })).collect::<Vec<_>>(),
    })
}

fn list_cordons(state: &mut AppState, _a: &Args<'_>) -> R<ToolResult> {
    Ok(ToolResult::json(cordons_json(state)))
}

fn cordon_box(a: &Args<'_>) -> R<Option<(glam::DVec3, glam::DVec3)>> {
    match (a.opt_vec3("min")?, a.opt_vec3("max")?) {
        (Some(p), Some(q)) => {
            let b = hp_geom::Aabb::new(p, q);
            if b.size().min_element() < 1.0 {
                return Err("the cordon box must be at least 1 unit on every axis".to_owned());
            }
            Ok(Some((b.min, b.max)))
        }
        (None, None) => Ok(None),
        _ => Err("give min and max together".to_owned()),
    }
}

fn add_cordon(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let (mins, maxs) = cordon_box(a)?.ok_or("min and max are required")?;
    let name = name_arg(a, "name")?.unwrap_or_else(|| "cordon".to_owned());
    let active = a.opt_bool("active")?.unwrap_or(true);
    let enable = a.opt_bool("enable")?.unwrap_or(true);
    let mut c = state.doc.scene().cordons().clone();
    if c.cordons.len() >= MAX_SETS {
        return Err(format!("the map has {MAX_SETS} cordons already"));
    }
    c.cordons.push(hp_vmf::Cordon { name, active, mins, maxs });
    if enable {
        c.active = true;
    }
    edit(state, "add_cordon", |doc| doc.set_cordons(c))?;
    Ok(ToolResult::json(cordons_json(state)))
}

fn edit_cordon(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let index = a.opt_u64("index", 0, 4095)?.ok_or("index is required")? as usize;
    let mut c = state.doc.scene().cordons().clone();
    if index >= c.cordons.len() {
        return Err(format!("the map has {} cordons (index {index} does not exist)", c.cordons.len()));
    }
    if a.opt_bool("delete")?.unwrap_or(false) {
        if a.get("name").is_some() || a.get("min").is_some() || a.get("max").is_some() || a.get("active").is_some() {
            return Err("delete takes no other change".to_owned());
        }
        c.cordons.remove(index);
        c.extra_boxes.retain(|b| b.cordon != index);
        for b in &mut c.extra_boxes {
            if b.cordon > index {
                b.cordon -= 1;
            }
        }
    } else {
        let k = &mut c.cordons[index];
        if let Some(n) = name_arg(a, "name")? {
            k.name = n;
        }
        if let Some((p, q)) = cordon_box(a)? {
            (k.mins, k.maxs) = (p, q);
        }
        if let Some(on) = a.opt_bool("active")? {
            k.active = on;
        }
    }
    edit(state, "edit_cordon", |doc| doc.set_cordons(c))?;
    Ok(ToolResult::json(cordons_json(state)))
}

fn set_cordons_enabled(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let on = a.opt_bool("enabled")?.ok_or("enabled is required")?;
    let mut c = state.doc.scene().cordons().clone();
    c.active = on;
    edit(state, "set_cordons_enabled", |doc| doc.set_cordons(c))?;
    Ok(ToolResult::json(cordons_json(state)))
}

#[cfg(test)]
mod tests {
    use hp_editor::ObjRef;
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    #[test]
    fn selection_tools() {
        let mut s = hp_app::testing::test_state();
        let r = ok(&mut s, "select_by_class", json!({ "pattern": "info_*" }));
        assert_eq!(r["matched"], 1);
        assert_eq!(s.doc.selected_objects(), vec![ObjRef::Entity(4)]);
        ok(&mut s, "select_by_class", json!({ "pattern": "light", "op": "add" }));
        assert_eq!(s.doc.selected_objects().len(), 2);
        let r = ok(&mut s, "select_by_material", json!({ "pattern": "concrete/*" }));
        assert_eq!(r["matched"], 1);
        ok(&mut s, "select_by_material", json!({ "pattern": "concrete/*", "faces": true }));
        assert_eq!(s.doc.selected_faces().len(), 6);
        ok(&mut s, "select_in_box", json!({ "min": [-300, 250, -10], "max": [300, 300, 200], "mode": "inside" }));
        assert_eq!(s.doc.selected_objects(), vec![ObjRef::Solid(3)]);
        ok(&mut s, "select_faces", json!({ "faces": [{ "solid": 2, "side": 0 }] }));
        assert_eq!(s.doc.selected_faces().len(), 1);
        ok(&mut s, "select_all", json!({}));
        assert!(s.doc.selected_objects().len() >= 5);
        err(&mut s, "select_by_class", json!({ "pattern": " " }));
        err(&mut s, "select_group", json!({}));
        let f = ok(&mut s, "frame_objects", json!({ "solid_ids": [3] }));
        assert!(f["camera_3d"]["position"].is_array());
        assert!(s.doc.undo_label().is_none());
    }

    #[test]
    fn display_colors() {
        let mut s = hp_app::testing::test_state();
        err(&mut s, "get_display_color", json!({}));
        let r = ok(&mut s, "get_display_color", json!({ "ids": [2, 3, 5] }));
        assert_eq!(r["state"], "default");
        assert_eq!(r["objects"][0]["color"], json!(null));
        assert_eq!(r["object_colors_2d"], true);
        assert_eq!(r["object_colors_3d"], false);
        s.doc.set_selection(&[ObjRef::Solid(2), ObjRef::Entity(6)]);
        let r = ok(&mut s, "set_display_color", json!({ "color": [240, 140, 40] }));
        assert_eq!(r["changed"], 3);
        assert_eq!(r["state"]["custom"], json!([240, 140, 40]));
        assert_eq!(s.doc.undo_label(), Some("MCP: set_display_color"));
        assert_eq!(s.doc.scene().solid(7).unwrap().solid.editor.color, [240, 140, 40]);
        let r = ok(&mut s, "get_display_color", json!({ "ids": [2, 3] }));
        assert_eq!(r["state"], "mixed");
        assert_eq!(r["objects"][0]["color"], json!([240, 140, 40]));
        ok(&mut s, "set_display_color", json!({ "ids": [2, 3], "random": true }));
        let r = ok(&mut s, "get_display_color", json!({ "ids": [2, 3] }));
        assert_ne!(r["objects"][0]["color"], r["objects"][1]["color"]);
        assert!(r["objects"][1]["color"].is_array());
        ok(&mut s, "set_display_color", json!({ "ids": [2, 3, 6], "reset": true }));
        assert_eq!(ok(&mut s, "get_display_color", json!({ "ids": [2, 3, 6] }))["state"], "default");
        err(&mut s, "set_display_color", json!({ "color": [1, 2, 3], "random": true }));
        err(&mut s, "set_display_color", json!({}));
        err(&mut s, "set_display_color", json!({ "color": [1, 2, 300] }));
        err(&mut s, "set_display_color", json!({ "ids": [999], "reset": true }));
        let steps = s.doc.history_len().0;
        assert!(s.doc.undo());
        assert_eq!(s.doc.history_len().0, steps - 1);
    }

    #[test]
    fn groups_and_visgroups() {
        let mut s = hp_app::testing::test_state();
        let g = ok(&mut s, "group_objects", json!({ "ids": [4, 5] }))["group"].as_u64().unwrap();
        let r = ok(&mut s, "list_groups", json!({}));
        assert_eq!(r["total"], 1);
        assert_eq!(r["groups"][0]["member_count"], 2);
        ok(&mut s, "select_group", json!({ "group": g }));
        assert_eq!(s.doc.selected_objects().len(), 2);
        ok(&mut s, "ungroup_objects", json!({ "groups": [g] }));
        assert_eq!(ok(&mut s, "list_groups", json!({}))["total"], 0);

        let v = ok(&mut s, "create_visgroup", json!({ "name": "Lights", "ids": [5], "color": [255, 0, 0] }))["id"]
            .as_u64()
            .unwrap();
        let child = ok(&mut s, "create_visgroup", json!({ "name": "Child", "parent": v }))["id"].as_u64().unwrap();
        let l = ok(&mut s, "list_visgroups", json!({}));
        assert_eq!(l["visgroups"][0]["members"], 1);
        assert_eq!(l["visgroups"][1]["parent"], v);
        ok(&mut s, "edit_visgroup", json!({ "id": child, "name": "Renamed", "parent": 0 }));
        err(&mut s, "edit_visgroup", json!({ "id": child }));
        ok(&mut s, "set_visgroup_members", json!({ "action": "add", "visgroup": child, "solid_ids": [2, 3] }));
        ok(&mut s, "set_visgroup_visible", json!({ "id": child, "visible": false }));
        let vis = ok(&mut s, "get_visibility", json!({ "solid_ids": [2] }));
        assert_eq!(vis["objects"][0]["visible"], false);
        ok(&mut s, "set_visgroup_visible", json!({ "all": true }));
        ok(&mut s, "set_visgroup_members", json!({ "action": "set", "visgroups": [], "solid_ids": [2] }));
        ok(&mut s, "move_to_visgroup", json!({ "ids": [4], "new_name": "Spawns", "hide": true }));
        assert!(!s.doc.is_visible(ObjRef::Entity(4)));
        ok(&mut s, "delete_visgroup", json!({ "id": v }));
        err(&mut s, "delete_visgroup", json!({ "id": v }));
        assert_eq!(s.doc.undo_label(), Some("MCP: delete_visgroup"));
    }

    #[test]
    fn hiding_and_cordons() {
        let mut s = hp_app::testing::test_state();
        ok(&mut s, "hide_objects", json!({ "solid_ids": [3] }));
        assert!(!s.doc.is_visible(ObjRef::Solid(3)));
        let r = ok(&mut s, "hidden_to_visgroup", json!({}));
        assert!(r["objects"].as_u64().unwrap() >= 1);
        ok(&mut s, "hide_objects", json!({ "unhide_all": true }));
        err(&mut s, "hide_objects", json!({ "unhide_all": true, "ids": [3] }));
        let mut fresh = hp_app::testing::test_state();
        let r = ok(&mut fresh, "isolate_objects", json!({ "solid_ids": [3] }));
        assert!(r["hidden"].as_u64().unwrap() >= 3);
        assert!(fresh.doc.is_visible(ObjRef::Solid(3)));
        assert!(!fresh.doc.is_visible(ObjRef::Solid(2)) && !fresh.doc.is_visible(ObjRef::Entity(5)));
        s.doc.select(&[ObjRef::Solid(2)], hp_editor::SelectOp::Replace);
        ok(&mut s, "hide_objects", json!({ "unselected": true }));
        assert!(!s.doc.is_visible(ObjRef::Solid(3)) && s.doc.is_visible(ObjRef::Solid(2)));

        let before = ok(&mut s, "list_cordons", json!({}))["cordons"].as_array().unwrap().len();
        for i in 0..before {
            ok(&mut s, "edit_cordon", json!({ "index": i, "active": false }));
        }
        let r = ok(&mut s, "add_cordon", json!({ "name": "Room", "min": [-64, -64, -64], "max": [64, 64, 64] }));
        assert_eq!(r["enabled"], true);
        assert_eq!(r["cordons"].as_array().unwrap().len(), before + 1);
        assert!(s.doc.is_cordoned_out(ObjRef::Solid(3)));
        ok(&mut s, "edit_cordon", json!({ "index": before, "max": [512, 512, 512], "min": [-512, -512, -512] }));
        assert!(!s.doc.is_cordoned_out(ObjRef::Solid(3)));
        ok(&mut s, "set_cordons_enabled", json!({ "enabled": false }));
        err(&mut s, "edit_cordon", json!({ "index": before + 5, "delete": true }));
        let r = ok(&mut s, "edit_cordon", json!({ "index": before, "delete": true }));
        assert_eq!(r["cordons"].as_array().unwrap().len(), before);
        err(&mut s, "add_cordon", json!({ "min": [0, 0, 0], "max": [0, 5, 5] }));
    }
}
