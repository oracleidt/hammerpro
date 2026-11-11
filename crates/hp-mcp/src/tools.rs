use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolKind {
    Read,
    Edit,
    Destructive,
    Editor,
}

#[derive(Clone, Copy, Debug)]
pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub kind: ToolKind,
    pub schema: fn() -> Value,
}

impl ToolSpec {
    pub fn definition(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": (self.schema)(),
            "annotations": {
                "title": self.title,
                "readOnlyHint": self.kind == ToolKind::Read,
                "destructiveHint": self.kind == ToolKind::Destructive,
                "idempotentHint": self.kind == ToolKind::Read,
                "openWorldHint": false,
            },
        })
    }
}

pub const MAX_LIST: u64 = 1000;
pub const MAX_VMF_TEXT: usize = 4 * 1024 * 1024;
pub const MAX_SCREENSHOT: u64 = 1920;
pub const MIN_SCREENSHOT: u64 = 64;

fn vec3(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": description })
}

fn ids(description: &str) -> Value {
    json!({ "type": "array", "items": { "type": "integer", "minimum": 1 }, "maxItems": 10000, "description": description })
}

fn solid_ids() -> Value {
    ids("Solid ids, when a solid shares its id with an entity (ids picks the entity first).")
}

fn integer(description: &str, min: u64, max: u64) -> Value {
    json!({ "type": "integer", "minimum": min, "maximum": max, "description": description })
}

fn string(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false })
}

fn empty() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn keyvalues(description: &str) -> Value {
    json!({ "type": "object", "additionalProperties": { "type": "string" }, "description": description })
}

fn limit() -> Value {
    integer("Most items returned (default 100).", 1, MAX_LIST)
}

fn offset() -> Value {
    integer("Items skipped first, for paging (default 0).", 0, u64::from(u32::MAX))
}

pub const EDITOR_TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "get_map_info",
        title: "Map information",
        description: "Describe the map open in the Hammer Pro window: file, unsaved changes, numbers of solids, \
            entities, faces and groups, map bounds, selection, undo and redo steps, game content and compile state. \
            Call it first.",
        kind: ToolKind::Read,
        schema: empty,
    },
    ToolSpec {
        name: "list_entities",
        title: "List entities",
        description: "List the entities of the map (not the world) with id, class, targetname, origin and number of \
            solids. Filters are case insensitive patterns with * and ?.",
        kind: ToolKind::Read,
        schema: || {
            object(
                json!({
                    "classname": string("Class pattern, for example \"prop_*\" or \"light\"."),
                    "targetname": string("Targetname pattern."),
                    "limit": limit(),
                    "offset": offset(),
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "get_entity",
        title: "Entity details",
        description: "Every keyvalue, output, solid id and the bounds of one entity.",
        kind: ToolKind::Read,
        schema: || object(json!({ "id": integer("Entity id.", 1, u64::from(u32::MAX)) }), &["id"]),
    },
    ToolSpec {
        name: "create_entity",
        title: "Create a point entity",
        description: "Create a point entity (info_player_start, light, prop_static with keyvalues.model, ...) at a \
            position. It becomes the selection. One undo step.",
        kind: ToolKind::Edit,
        schema: || {
            object(
                json!({
                    "classname": string("Entity class, for example \"prop_physics\"."),
                    "origin": vec3("Position [x, y, z] in Source units."),
                    "angles": vec3("Orientation [pitch, yaw, roll] in degrees (optional)."),
                    "keyvalues": keyvalues("Other keyvalues, for example {\"model\": \"models/props_c17/oildrum001.mdl\"}."),
                }),
                &["classname", "origin"],
            )
        },
    },
    ToolSpec {
        name: "set_entity_keyvalues",
        title: "Edit entity keyvalues",
        description: "Set or remove keyvalues of one entity (the world uses its id from get_map_info: skyname, \
            detailmaterial...). Setting \"classname\" changes the class of an entity. One undo step.",
        kind: ToolKind::Edit,
        schema: || {
            object(
                json!({
                    "id": integer("Entity id.", 1, u64::from(u32::MAX)),
                    "set": keyvalues("Keyvalues to set."),
                    "remove": { "type": "array", "items": { "type": "string" }, "maxItems": 1000, "description": "Keys to remove." },
                }),
                &["id"],
            )
        },
    },
    ToolSpec {
        name: "delete_objects",
        title: "Delete objects",
        description: "Delete entities and solids by id. Deleting a brush entity deletes its solids. One undo step.",
        kind: ToolKind::Destructive,
        schema: || object(json!({ "ids": ids("Entity or solid ids."), "solid_ids": solid_ids() }), &[]),
    },
    ToolSpec {
        name: "list_solids",
        title: "List solids",
        description: "List solids (brushes) with id, owner entity, bounds and materials. Filter by material pattern, \
            owner entity (0 for world brushes) or a box the bounds must touch.",
        kind: ToolKind::Read,
        schema: || {
            object(
                json!({
                    "material": string("Material pattern, for example \"tools/*\"."),
                    "entity": integer("Owner entity id, 0 for world brushes.", 0, u64::from(u32::MAX)),
                    "min": vec3("Box minimum [x, y, z] (with max)."),
                    "max": vec3("Box maximum [x, y, z] (with min)."),
                    "limit": limit(),
                    "offset": offset(),
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "create_brush",
        title: "Create a brush",
        description: "Create a brush (block, wedge, cylinder, spike, sphere, arch, torus, stairs, spiral_stairs, ramp, pillar or doorway) filling a box, with a \
            material. With entity_class the brushes become a brush entity (func_detail, trigger_multiple...). The new \
            objects become the selection. One undo step.",
        kind: ToolKind::Edit,
        schema: || {
            object(
                json!({
                    "shape": { "type": "string", "enum": ["block", "wedge", "cylinder", "spike", "sphere", "arch", "torus", "stairs", "spiral_stairs", "ramp", "pillar", "doorway"], "description": "Shape (default block)." },
                    "min": vec3("Box minimum [x, y, z]."),
                    "max": vec3("Box maximum [x, y, z]."),
                    "material": string("Material of every face (default: the editor's current material)."),
                    "sides": integer("Sides of round shapes (default 8).", 3, 128),
                    "entity_class": string("Brush entity class to put the brushes in (optional)."),
                    "keyvalues": keyvalues("Keyvalues of the brush entity (with entity_class)."),
                }),
                &["min", "max"],
            )
        },
    },
    ToolSpec {
        name: "apply_material",
        title: "Apply a material",
        description: "Apply a material to every face of objects, or to single faces (solid id and side index, as \
            listed by list_solids). One undo step.",
        kind: ToolKind::Edit,
        schema: || {
            object(
                json!({
                    "material": string("Material name, for example \"concrete/concretefloor001a\"."),
                    "ids": ids("Objects whose faces all get the material."),
                    "solid_ids": solid_ids(),
                    "faces": {
                        "type": "array",
                        "maxItems": 10000,
                        "items": object(json!({ "solid": integer("Solid id.", 1, u64::from(u32::MAX)), "side": integer("Side index.", 0, 1000) }), &["solid", "side"]),
                        "description": "Single faces.",
                    },
                }),
                &["material"],
            )
        },
    },
    ToolSpec {
        name: "move_objects",
        title: "Move objects",
        description: "Move objects by an offset (texture lock follows the editor setting). One undo step.",
        kind: ToolKind::Edit,
        schema: || {
            object(
                json!({ "ids": ids("Entity or solid ids."), "solid_ids": solid_ids(), "offset": vec3("Offset [dx, dy, dz].") }),
                &["offset"],
            )
        },
    },
    ToolSpec {
        name: "rotate_objects",
        title: "Rotate objects",
        description: "Rotate objects around an axis through a pivot (default: the center of their bounds). Entities \
            turn with their angles; lights (light_spot, light_environment, light_directional) also get the Pitch key \
            of their new direction (positive is up). One undo step.",
        kind: ToolKind::Edit,
        schema: || {
            object(
                json!({
                    "ids": ids("Entity or solid ids."),
                    "solid_ids": solid_ids(),
                    "axis": { "type": "string", "enum": ["x", "y", "z"], "description": "Rotation axis (default z)." },
                    "degrees": { "type": "number", "minimum": -360, "maximum": 360, "description": "Angle, counterclockwise seen from the positive axis." },
                    "pivot": vec3("Pivot [x, y, z] (optional)."),
                }),
                &["degrees"],
            )
        },
    },
    ToolSpec {
        name: "select_objects",
        title: "Select objects",
        description: "Change the selection of the editor. Like a click, the change is one undo step named \
            Selection when the setting edit.undo_restores_selection is on (the default).",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "ids": ids("Entity or solid ids."),
                    "solid_ids": solid_ids(),
                    "mode": { "type": "string", "enum": ["replace", "add", "remove"], "description": "Default replace." },
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "get_selection",
        title: "Selection",
        description: "The selected objects (ids and kinds), selected faces and the selection bounds.",
        kind: ToolKind::Read,
        schema: empty,
    },
    ToolSpec {
        name: "clear_selection",
        title: "Clear the selection",
        description: "Deselect every object and every face.",
        kind: ToolKind::Editor,
        schema: empty,
    },
    ToolSpec {
        name: "search_materials",
        title: "Search materials",
        description: "Search the materials of the mounted game content by name (text or pattern with * and ?).",
        kind: ToolKind::Read,
        schema: || {
            object(
                json!({ "query": string("Part of the name, or a pattern."), "limit": integer("Most results (default 50).", 1, 500) }),
                &["query"],
            )
        },
    },
    ToolSpec {
        name: "search_models",
        title: "Search models",
        description: "Search the models (.mdl) of the mounted game content by path (text or pattern with * and ?).",
        kind: ToolKind::Read,
        schema: || {
            object(
                json!({ "query": string("Part of the path, or a pattern."), "limit": integer("Most results (default 50).", 1, 500) }),
                &["query"],
            )
        },
    },
    ToolSpec {
        name: "get_vmf_text",
        title: "Objects as VMF text",
        description: "The VMF text of objects (default: the selection), like the editor clipboard. Edit it and give \
            it back to paste_vmf_text.",
        kind: ToolKind::Read,
        schema: || object(json!({ "ids": ids("Entity or solid ids."), "solid_ids": solid_ids() }), &[]),
    },
    ToolSpec {
        name: "paste_vmf_text",
        title: "Paste VMF text",
        description: "Add the objects of a VMF text (world solids, entities, groups) to the map with new ids, moved \
            by an optional offset. They become the selection. One undo step.",
        kind: ToolKind::Edit,
        schema: || {
            object(
                json!({ "text": string("VMF text (at most 4 MB)."), "offset": vec3("Offset [dx, dy, dz] (default none).") }),
                &["text"],
            )
        },
    },
    ToolSpec {
        name: "check_problems",
        title: "Check for problems",
        description: "Run the map checks (invalid solids, leaks prone entities, missing targets, unknown classes, \
            redundant brushwork...) and list the problems. category redundant keeps only the duplicate brushes, the \
            brushes inside identical solid brushes and the stacked duplicate entities (fix_problems or \
            find_duplicate_brushes delete them).",
        kind: ToolKind::Read,
        schema: || {
            object(
                json!({
                    "limit": limit(),
                    "category": { "type": "string", "enum": ["all", "redundant"], "description": "Which problems (default all)." },
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "compile_map",
        title: "Compile the map",
        description: "Start compiling the saved map with a preset (Hammer Pro compiles it itself: geometry and \
            visibility, then lighting). Returns at once: poll get_compile_status. The game is only started when \
            launch_game is true.",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "preset": { "type": "string", "enum": ["draft", "normal", "final"], "description": "Quality preset (default: the editor's)." },
                    "launch_game": { "type": "boolean", "description": "Run the map in the game when done (default false)." },
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "get_compile_status",
        title: "Compile status",
        description: "Whether a compile runs, its step and progress (while the lighting runs, the step names its \
            stage, the stage percent and the time left), and the result of the last compile.",
        kind: ToolKind::Read,
        schema: empty,
    },
    ToolSpec {
        name: "save_map",
        title: "Save the map",
        description: "Save the map to its file, or as a new .vmf in the folder of the current map file (a file name \
            like \"my_map_v2.vmf\"). Replacing another existing file needs overwrite true.",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "path": string("New .vmf file name, in the folder of the current map (optional)."),
                    "overwrite": { "type": "boolean", "description": "Replace an existing file (default false)." },
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "open_map",
        title: "Open a map",
        description: "Open a .vmf file in the editor (in a new tab, or after the user answers the unsaved changes \
            question). Returns at once: call get_map_info to see it.",
        kind: ToolKind::Editor,
        schema: || object(json!({ "path": string("Full path of the .vmf file.") }), &["path"]),
    },
    ToolSpec {
        name: "undo",
        title: "Undo",
        description: "Undo the last edit (any edit, the user's included). With the setting \
            edit.undo_restores_selection (on by default) a selection change is a step too: undo brings back the \
            previous selection, faces included.",
        kind: ToolKind::Destructive,
        schema: empty,
    },
    ToolSpec {
        name: "redo",
        title: "Redo",
        description: "Redo the last undone edit.",
        kind: ToolKind::Edit,
        schema: empty,
    },
    ToolSpec {
        name: "set_camera",
        title: "Place the cameras",
        description: "Place the 3D camera (position, yaw and pitch in degrees, or a point to look at) and center or \
            zoom the 2D views.",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "position": vec3("3D camera position [x, y, z]."),
                    "yaw": { "type": "number", "minimum": -360, "maximum": 360, "description": "Degrees, 0 looks along +X, 90 along +Y." },
                    "pitch": { "type": "number", "minimum": -89, "maximum": 89, "description": "Degrees, positive looks up." },
                    "look_at": vec3("Point the 3D camera looks at [x, y, z]."),
                    "center_2d": vec3("Point the 2D views center on [x, y, z]."),
                    "zoom_2d": { "type": "number", "minimum": 0.002, "maximum": 64, "description": "2D zoom in screen points per unit." },
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "screenshot_view",
        title: "Screenshot of a view",
        description: "Render the 3D view or a 2D view (top, front, side) with the current cameras and return it as a \
            PNG image. The 2D logical view (the I/O schematic a quadrant can show) is not a render: look at it \
            with screenshot_window.",
        kind: ToolKind::Read,
        schema: || {
            object(
                json!({
                    "view": { "type": "string", "enum": ["3d", "top", "front", "side"], "description": "Default 3d." },
                    "width": integer("Width in pixels (default 1280).", MIN_SCREENSHOT, MAX_SCREENSHOT),
                    "height": integer("Height in pixels (default 720).", MIN_SCREENSHOT, MAX_SCREENSHOT),
                }),
                &[],
            )
        },
    },
];

pub const BRIDGE_TOOLS: &[ToolSpec] = &[
    ToolSpec {
        name: "list_instances",
        title: "Hammer Pro windows",
        description: "List the Hammer Pro windows that have MCP switched on (process id, map, last focus) and the \
            one the tools talk to.",
        kind: ToolKind::Read,
        schema: empty,
    },
    ToolSpec {
        name: "use_instance",
        title: "Choose a Hammer Pro window",
        description: "Send the next tool calls to the Hammer Pro window with this process id (from list_instances).",
        kind: ToolKind::Editor,
        schema: || object(json!({ "pid": integer("Process id.", 1, u64::from(u32::MAX)) }), &["pid"]),
    },
];

pub fn editor_tool_definitions() -> Vec<Value> {
    EDITOR_TOOLS.iter().map(ToolSpec::definition).collect()
}

pub fn bridge_tool_definitions() -> Vec<Value> {
    EDITOR_TOOLS.iter().chain(BRIDGE_TOOLS).map(ToolSpec::definition).collect()
}

pub fn editor_tool(name: &str) -> Option<&'static ToolSpec> {
    EDITOR_TOOLS.iter().find(|t| t.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_are_well_formed() {
        let mut names = std::collections::HashSet::new();
        for t in EDITOR_TOOLS.iter().chain(BRIDGE_TOOLS) {
            assert!(names.insert(t.name), "duplicate {}", t.name);
            assert!(!t.name.is_empty() && t.name.len() <= 64);
            assert!(t.name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'), "{}", t.name);
            assert!(t.description.len() > 20, "{}", t.name);
            assert!(!t.description.contains("  "), "{}: double space", t.name);
            let d = t.definition();
            let schema = &d["inputSchema"];
            assert_eq!(schema["type"], "object", "{}", t.name);
            assert!(schema["properties"].is_object(), "{}", t.name);
            for r in schema.get("required").and_then(Value::as_array).into_iter().flatten() {
                let key = r.as_str().unwrap();
                assert!(schema["properties"].get(key).is_some(), "{}: {key}", t.name);
            }
            let text = serde_json::to_string(&d).unwrap();
            assert!(!text.contains("oneOf") && !text.contains("anyOf"), "{}", t.name);
            assert!(!text.contains("\"type\":["), "{}", t.name);
            for c in ['\u{2013}', '\u{2014}'] {
                assert!(!text.contains(c), "{}", t.name);
            }
        }
        assert_eq!(bridge_tool_definitions().len(), EDITOR_TOOLS.len() + BRIDGE_TOOLS.len());
        assert!(editor_tool("create_brush").is_some());
        assert!(editor_tool("list_instances").is_none());
        let spec = editor_tool("delete_objects").unwrap().definition();
        assert_eq!(spec["annotations"]["destructiveHint"], true);
        assert_eq!(editor_tool("get_map_info").unwrap().definition()["annotations"]["readOnlyHint"], true);
    }
}
