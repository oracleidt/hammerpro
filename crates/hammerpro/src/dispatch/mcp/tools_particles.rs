use std::path::{Path, PathBuf};

use hp_app::AppState;
use hp_mcp::protocol::ToolResult;
use hp_pcf::dmx::{Reference, Value as DmxValue};
use hp_pcf::editor::{Category, Document, Preset, Severity};
use hp_ui_lab::particle_editor::{EditorSession, PreviewOptions};
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{AppFn, Args, Outcome, R, edit, encode_png};

const CATEGORIES: &[&str] = &["emitters", "initializers", "operators", "forces", "constraints", "renderers"];
const TYPES: &[&str] = &[
    "element",
    "int",
    "float",
    "bool",
    "string",
    "color",
    "vec2",
    "vec3",
    "vec4",
    "elements",
    "ints",
    "floats",
    "vec3s",
    "strings",
    "external",
    "references",
    "raw",
];
const OPERATIONS: &[&str] = &[
    "show",
    "close",
    "new",
    "open",
    "import",
    "save",
    "export",
    "validate",
    "create",
    "rename",
    "duplicate",
    "delete",
    "select",
    "preset",
    "add_operator",
    "remove_operator",
    "move_operator",
    "copy_operator",
    "paste_operator",
    "set_parameter",
    "remove_parameter",
    "add_child",
    "remove_child",
    "undo",
    "redo",
    "preview",
    "assign",
];

domain! { "particle editor";
    get_particle_document: Read, "Read the particle editor document",
        "The integrated particle editor: systems with element ids, category operator ids and children, selected system, history, source and unsaved state, preview camera and controls. Give element to read its typed parameters (otherwise the selected element). Systems are paginated with offset and limit; element ids remain stable during editing.",
        { "element": integer("DMX element id to inspect.", 0, 1000000), "offset": offset(), "limit": limit() }, [];
    list_particle_operators: Read, "Particle operator catalog",
        "Built in Source particle operator templates and their typed default parameters, grouped into emitters, initializers, operators, forces, constraints and renderers. Custom functions and parameters are accepted by particle_editor and preserved in PCF even if the preview reports them as unsupported.",
        { "category": choice("Restrict the operator category.", CATEGORIES), "query": string("Case insensitive function-name filter.") }, [];
    particle_editor: Editor, "Edit particle systems and preview them",
        "Operate the integrated PCF editor. show/close opens or closes its window. new/open/preset replaces the document (confirm required when unsaved). open/import reads file from mounted content or an absolute .pcf path. save writes an absolute .pcf path; export writes to an absolute addon folder with name ending .pcf, returning manifest and Lua. create/rename/duplicate uses name and optional system (default selected). delete/select targets system. add_operator uses category/function; remove_operator/move_operator uses category/index and to. copy_operator uses element; paste_operator uses category. set_parameter uses element/key/type/value_json (JSON-encoded value); remove_parameter uses element/key. add_child uses child/delay; remove_child uses index. undo/redo covers document edits. preview changes playback/camera/grid/background/control points/restart. assign sets effect_name on selected info_particle_system entities as one map undo step. validate includes unsupported preview operators and missing mounted materials. Raw values use value_json {kind: DMX type id, data: byte array}; references are element ids, null, or external-reference strings. All unknown Source operators and parameters remain editable.",
        {
            "op": choice("Operation.", OPERATIONS), "system": integer("System element id (default selected).", 0, 1000000),
            "element": integer("Parameter or clipboard element id (default selected element).", 0, 1000000),
            "name": string("System name, preset fire/smoke/sparks/rain/explosion, or exported .pcf file name."),
            "file": string("Mounted content path or absolute .pcf path to open/import."),
            "path": string("Absolute output .pcf path for save, or addon folder for export."),
            "confirm": boolean("Discard unsaved document changes when replacing or closing it."),
            "category": choice("Operator category.", CATEGORIES), "function": string("Source operator function name, including custom functions."),
            "index": integer("Operator or child index, zero based.", 0, 1000000), "to": integer("Destination operator index.", 0, 1000000),
            "key": string("DMX parameter name, including spaces."), "type": choice("Parameter storage type.", TYPES),
            "value_json": string("JSON-encoded value, such as 25, [1,2,3], true, or a quoted string. Raw is {\"kind\":14,\"data\":[...]}; references use ids, null, or external strings."),
            "child": integer("Child system element id.", 0, 1000000), "delay": number("Child start delay in seconds.", 0.0, 36000.0),
            "paused": boolean("Pause preview simulation."), "speed": number("Preview time multiplier (pause with paused true).", 0.05, 4.0),
            "yaw": number("Orbit yaw in degrees.", -36000.0, 36000.0), "pitch": number("Orbit pitch in degrees.", -85.0, 85.0),
            "zoom": number("Preview zoom factor.", 0.1, 10.0), "grid": boolean("Show preview grid."),
            "background": json!({"type":"array","minItems":3,"maxItems":3,"items":{"type":"number","minimum":0,"maximum":1},"description":"Preview background RGB from zero to one."}),
            "points": json!({"type":"array","minItems":1,"maxItems":64,"items":{"type":"array","minItems":3,"maxItems":3,"items":{"type":"number","minimum":-131072,"maximum":131072}},"description":"Control point positions, including control point zero (1 to 64 positions)."}),
            "restart": boolean("Restart preview from its initial state."),
        }, ["op"];
    capture_particle_preview: Read, "Capture the particle editor preview",
        "PNG of the particle editor's shared 3D renderer output. Open the editor and allow a frame to render before capture (particle_editor op show, then wait_frames). Includes the current simulation, camera, materials and grid.",
        {}, [];
}

fn get_particle_document(_state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    let element = a.opt_u64("element", 0, 1000000)?.map(|n| n as usize);
    let offset = a.opt_u64("offset", 0, 1000000)?.unwrap_or(0) as usize;
    let limit = a.opt_u64("limit", 1, hp_mcp::tools::MAX_LIST)?.unwrap_or(200) as usize;
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, _ctx| {
        app.flush_particle_edits();
        let Ok(session) = app.pkg.lab.particle_editor.lock() else {
            return ToolResult::error("the particle editor is busy");
        };
        match document_json(&session, element, offset, limit) {
            Ok(value) => ToolResult::json(value),
            Err(e) => ToolResult::error(e),
        }
    }))))
}

fn list_particle_operators(_state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let category = a.opt_enum("category", CATEGORIES)?;
    let query = a.opt_str("query", 255)?.unwrap_or("").to_ascii_lowercase();
    let out: Vec<Value> = hp_ui_lab::particle_editor::operator_catalog().into_iter().filter(|(c,f,_)| category.as_deref().is_none_or(|given| given == c) && f.to_ascii_lowercase().contains(&query)).map(|(category,function,params)| {
        json!({"category":category,"function":function,"parameters":params.into_iter().map(|(key,value)| json!({"key":key,"value":value_json(&value)})).collect::<Vec<_>>() })
    }).collect();
    Ok(ToolResult::json(json!({"operators":out,"custom_functions":true,"parameter_types":TYPES})))
}

fn capture_particle_preview(_state: &mut AppState, _a: &Args<'_>) -> R<Outcome> {
    Ok(Outcome::AppFn(AppFn(Box::new(|app, _ctx| {
        app.flush_particle_edits();
        let result = (|| {
            let gfx = app.state.gfx.as_mut().ok_or("the 3D renderer is not ready")?;
            let (w, h, pixels) = gfx
                .renderer
                .read_view_rgba8(hp_ui_lab::particle_editor::EDITOR_VIEW_ID)
                .map_err(|e| format!("the particle preview has not rendered yet: {e}"))?;
            let png = encode_png(w, h, pixels)?;
            Ok::<_, String>(ToolResult::png(&png, format!("Particle editor preview, {w}x{h}")))
        })();
        result.unwrap_or_else(ToolResult::error)
    }))))
}

fn particle_editor(_state: &mut AppState, a: &Args<'_>) -> R<Outcome> {
    validate_editor_arguments(a)?;
    let values = Value::Object(a.map.clone());
    Ok(Outcome::AppFn(AppFn(Box::new(move |app, ctx| {
        app.flush_particle_edits();
        let handle = app.pkg.lab.particle_editor.clone();
        let Ok(mut session) = handle.lock() else { return ToolResult::error("the particle editor is busy") };
        let a = Args { map: values.as_object().expect("object arguments") };
        let result = apply_editor(&mut app.state, &mut session, &a)
            .map(ToolResult::json)
            .unwrap_or_else(|e| ToolResult::error(format!("particle_editor: {e}")));
        ctx.request_repaint();
        result
    }))))
}

fn validate_editor_arguments(a: &Args<'_>) -> R<()> {
    let op = a.opt_enum("op", OPERATIONS)?.ok_or("op is required")?;
    a.opt_bool("confirm")?;
    for key in ["system", "element", "index", "to", "child"] {
        a.opt_u64(key, 0, 1000000)?;
    }
    for (key, max) in
        [("name", 255), ("file", 32768), ("path", 32768), ("function", 255), ("key", 1024), ("value_json", 1048576)]
    {
        a.opt_str(key, max)?;
    }
    a.opt_enum("category", CATEGORIES)?;
    a.opt_enum("type", TYPES)?;
    a.opt_f64("delay", 0.0, 36000.0)?;
    update_preview(&mut PreviewOptions::default(), a)?;
    match op.as_str() {
        "open" | "import" => {
            let file = a.str("file", 32768)?;
            if !Path::new(file).extension().is_some_and(|e| e.eq_ignore_ascii_case("pcf")) {
                return Err("the input must be a .pcf file".into());
            }
        }
        "save" => {
            output_path(a, true)?;
        }
        "export" => {
            output_path(a, false)?;
            a.str("name", 255)?;
        }
        "create" | "rename" | "duplicate" => {
            let name = a.str("name", 255)?;
            if name.trim().is_empty() || name.contains('\0') {
                return Err("the system name must be nonempty and contain no NUL characters".into());
            }
        }
        "preset" => {
            let name = a.str("name", 64)?;
            if !Preset::ALL.into_iter().any(|p| p.name().eq_ignore_ascii_case(name)) {
                return Err("unknown particle preset".into());
            }
        }
        "add_operator" => {
            category(a)?;
            let function = a.str("function", 255)?;
            if function.trim().is_empty() || function.contains('\0') {
                return Err("the operator function must be nonempty and contain no NUL characters".into());
            }
        }
        "remove_operator" | "move_operator" => {
            category(a)?;
            index(a, "index")?;
            if op == "move_operator" {
                index(a, "to")?;
            }
        }
        "paste_operator" => {
            category(a)?;
        }
        "set_parameter" => {
            a.str("key", 1024)?;
            let value: Value =
                serde_json::from_str(a.str("value_json", 1048576)?).map_err(|e| format!("invalid value_json: {e}"))?;
            decode_value(a.str("type", 32)?, &value)?;
        }
        "remove_parameter" => {
            a.str("key", 1024)?;
        }
        "add_child" => {
            index(a, "child")?;
        }
        "remove_child" => {
            index(a, "index")?;
        }
        _ => {}
    }
    Ok(())
}

fn preview_json(p: &PreviewOptions) -> Value {
    json!({"paused":p.paused,"speed":p.speed,"yaw":p.yaw,"pitch":p.pitch,"zoom":p.zoom,"grid":p.grid,"background":p.background,"points":p.points,"restart":p.restart,"particles":p.particles,"cpu_ms":p.cpu_ms})
}

fn document_json(session: &EditorSession, element: Option<usize>, offset: usize, limit: usize) -> R<Value> {
    let ids = session.doc.systems();
    let systems: Vec<Value> = ids.iter().skip(offset).take(limit).map(|&id| {
        let e = &session.doc.dmx().elements[id];
        let categories: serde_json::Map<String,Value> = Category::ALL.into_iter().map(|c| (c.as_str().into(),json!(session.doc.operators(id,c)))).collect();
        json!({"id":id,"name":e.name,"material":e.str("material"),"max_particles":e.int("max_particles"),"categories":categories,"children":session.doc.children(id)})
    }).collect();
    let selected_element = element.or(session.selected_element);
    let inspected = selected_element.map(|id| {
        let e = session.doc.element(id).ok_or_else(||format!("element {id} does not exist"))?;
        Ok::<_,String>(json!({"id":id,"kind":e.kind,"name":e.name,"parameters":e.attrs.iter().map(|(key,value)|json!({"key":key,"value":value_json(value)})).collect::<Vec<_>>()}))
    }).transpose()?;
    Ok(
        json!({"open":session.open,"source":session.source,"dirty":session.dirty(),"revision":session.revision,"selected":session.selected,"selected_element":session.selected_element,"systems":systems,"total":ids.len(),"offset":offset,"element":inspected,"can_undo":session.doc.can_undo(),"can_redo":session.doc.can_redo(),"undo":session.doc.undo_label(),"redo":session.doc.redo_label(),"clipboard":session.doc.has_clipboard(),"preview":preview_json(&session.preview)}),
    )
}

fn selected_system(session: &EditorSession, a: &Args<'_>) -> R<usize> {
    let id = a
        .opt_u64("system", 0, 1000000)?
        .map(|n| n as usize)
        .or(session.selected)
        .ok_or("select a particle system first")?;
    session.doc.system_name(id).ok_or_else(|| format!("element {id} is not a particle system"))?;
    Ok(id)
}

fn selected_element(session: &EditorSession, a: &Args<'_>) -> R<usize> {
    a.opt_u64("element", 0, 1000000)?
        .map(|n| n as usize)
        .or(session.selected_element)
        .ok_or_else(|| "select a particle element first".into())
}

fn category(a: &Args<'_>) -> R<Category> {
    Category::parse(a.str("category", 64)?).ok_or_else(|| "unknown operator category".into())
}

fn index(a: &Args<'_>, key: &str) -> R<usize> {
    a.opt_u64(key, 0, 1000000)?.map(|n| n as usize).ok_or_else(|| format!("{key} is required"))
}

fn output_path(a: &Args<'_>, pcf: bool) -> R<PathBuf> {
    let path = super::semantic::local_path(a.str("path", 32768)?)?;
    if !path.is_absolute() || (pcf && !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pcf"))) {
        return Err("output path must be absolute and a PCF output must end in .pcf".into());
    }
    Ok(path)
}

fn read_pcf(state: &AppState, file: &str) -> R<Vec<u8>> {
    if !Path::new(file).extension().is_some_and(|e| e.eq_ignore_ascii_case("pcf")) {
        return Err("the input must be a .pcf file".into());
    }
    let bytes = if Path::new(file).is_absolute() {
        super::semantic::local_path(file)?;
        let meta = std::fs::metadata(file).map_err(|e| e.to_string())?;
        if !meta.is_file() || meta.len() > hp_pcf::MAX_FILE_SIZE as u64 {
            return Err("the PCF must be a file no larger than 64 MB".into());
        }
        std::fs::read(file).map_err(|e| e.to_string())?
    } else {
        if Path::new(file).components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            return Err("mounted PCF paths cannot traverse parent folders".into());
        }
        state.assets.fs().and_then(|fs| fs.read(file)).ok_or_else(|| format!("particle file is not mounted: {file}"))?
    };
    if bytes.len() > hp_pcf::MAX_FILE_SIZE {
        return Err("the PCF is larger than 64 MB".into());
    }
    Ok(bytes)
}

fn apply_editor(state: &mut AppState, s: &mut EditorSession, a: &Args<'_>) -> R<Value> {
    let op = a.str("op", 64)?;
    if matches!(op, "new" | "open" | "preset" | "close") && s.dirty() && !a.opt_bool("confirm")?.unwrap_or(false) {
        return Err("the document has unsaved changes; give confirm: true to discard them".into());
    }
    let mut result = json!({"operation":op});
    let revision = s.doc.revision();
    match op {
        "show" => s.open = true,
        "close" => s.close_discarding_changes(),
        "new" => {
            s.new_document();
            s.open = true;
        }
        "open" => {
            let file = a.str("file", 32768)?;
            let bytes = read_pcf(state, file)?;
            s.load_bytes(file, &bytes)?;
        }
        "import" => {
            let file = a.str("file", 32768)?;
            let source = Document::parse(&read_pcf(state, file)?).map_err(|e| e.to_string())?;
            result["imported"] = json!(s.doc.import(&source).map_err(|e| e.to_string())?);
        }
        "save" => {
            let path = output_path(a, true)?;
            s.save(&path)?;
            result["path"] = json!(path.display().to_string());
        }
        "export" => {
            let path = output_path(a, false)?;
            let name = a.str("name", 255)?;
            let exported = s.doc.export_addon(&path, name).map_err(|e| e.to_string())?;
            result["pcf"] = json!(exported.pcf.display().to_string());
            result["manifest"] = json!(exported.manifest.display().to_string());
            result["lua"] = json!(exported.lua);
        }
        "validate" => {
            let mut issues = s.doc.validate();
            if let Some(fs) = state.assets.fs() {
                issues.extend(s.doc.validate_materials(|material| fs.material(material).is_some()));
            }
            result["valid"] = json!(issues.iter().all(|i| i.severity != Severity::Error));
            result["issues"]=json!(issues.into_iter().map(|i|json!({"severity":if i.severity==Severity::Error{"error"}else{"warning"},"element":i.element,"attribute":i.attribute,"message":i.message})).collect::<Vec<_>>());
        }
        "preset" => {
            let name = a.str("name", 64)?;
            let preset = Preset::ALL
                .into_iter()
                .find(|p| p.name().eq_ignore_ascii_case(name))
                .ok_or("unknown particle preset")?;
            s.doc = Document::preset(preset);
            s.source = None;
            s.changed();
            s.selected = s.doc.systems().first().copied();
            s.selected_element = s.selected;
            s.open = true;
        }
        "create" => {
            let id = s.doc.create_system(a.str("name", 255)?).map_err(|e| e.to_string())?;
            s.selected = Some(id);
            s.selected_element = Some(id);
            result["system"] = json!(id);
        }
        "rename" => {
            let system = selected_system(s, a)?;
            s.doc.rename_system(system, a.str("name", 255)?).map_err(|e| e.to_string())?;
        }
        "duplicate" => {
            let system = selected_system(s, a)?;
            let id = s.doc.duplicate_system(system, a.str("name", 255)?).map_err(|e| e.to_string())?;
            s.selected = Some(id);
            s.selected_element = Some(id);
            result["system"] = json!(id);
        }
        "delete" => {
            let system = selected_system(s, a)?;
            s.doc.delete_system(system).map_err(|e| e.to_string())?;
        }
        "select" => {
            let system = selected_system(s, a)?;
            let element = a.opt_u64("element", 0, 1000000)?.map(|id| id as usize);
            if element.is_some_and(|id| s.doc.element(id).is_none()) {
                return Err("element does not exist".into());
            }
            s.select(system);
            if let Some(element) = element {
                s.selected_element = Some(element);
            }
        }
        "add_operator" => {
            let system = selected_system(s, a)?;
            let category = category(a)?;
            let function = a.str("function", 255)?;
            let params = hp_ui_lab::particle_editor::operator_templates(category)
                .into_iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(function))
                .map(|(_, params)| params.into_iter().map(|(key, value)| (key.into(), value)).collect())
                .unwrap_or_default();
            let id = s.doc.add_operator_with_params(system, category, function, params).map_err(|e| e.to_string())?;
            s.selected_element = Some(id);
            result["element"] = json!(id);
        }
        "remove_operator" => {
            let system = selected_system(s, a)?;
            s.doc.remove_operator(system, category(a)?, index(a, "index")?).map_err(|e| e.to_string())?;
            s.selected_element = Some(system);
        }
        "move_operator" => {
            let system = selected_system(s, a)?;
            s.doc
                .move_operator(system, category(a)?, index(a, "index")?, index(a, "to")?)
                .map_err(|e| e.to_string())?;
        }
        "copy_operator" => {
            let element = selected_element(s, a)?;
            s.doc.copy_operator(element).map_err(|e| e.to_string())?;
        }
        "paste_operator" => {
            let system = selected_system(s, a)?;
            let id = s.doc.paste_operator(system, category(a)?).map_err(|e| e.to_string())?;
            s.selected_element = Some(id);
            result["element"] = json!(id);
        }
        "set_parameter" => {
            let element = selected_element(s, a)?;
            let value: Value =
                serde_json::from_str(a.str("value_json", 1048576)?).map_err(|e| format!("invalid value_json: {e}"))?;
            let value = decode_value(a.str("type", 32)?, &value)?;
            s.doc.set_attr(element, a.str("key", 1024)?, value).map_err(|e| e.to_string())?;
        }
        "remove_parameter" => {
            let element = selected_element(s, a)?;
            s.doc.remove_attr(element, a.str("key", 1024)?).map_err(|e| e.to_string())?;
        }
        "add_child" => {
            let system = selected_system(s, a)?;
            let child = index(a, "child")?;
            let id = s
                .doc
                .add_child(system, child, a.opt_f64("delay", 0.0, 36000.0)?.unwrap_or(0.0) as f32)
                .map_err(|e| e.to_string())?;
            s.selected_element = Some(id);
            result["element"] = json!(id);
        }
        "remove_child" => {
            let system = selected_system(s, a)?;
            s.doc.remove_child(system, index(a, "index")?).map_err(|e| e.to_string())?;
            s.selected_element = Some(system);
        }
        "undo" => {
            result["changed"] = json!(s.doc.undo());
        }
        "redo" => {
            result["changed"] = json!(s.doc.redo());
        }
        "preview" => {
            update_preview(&mut s.preview, a)?;
            s.open = true;
        }
        "assign" => {
            result["entities_changed"] = json!(assign(state, s, a)?);
        }
        _ => return Err("unknown particle editor operation".into()),
    }
    if s.doc.revision() != revision && !matches!(op, "new" | "open" | "preset" | "close") {
        s.changed();
    }
    result["dirty"] = json!(s.dirty());
    result["selected"] = json!(s.selected);
    result["revision"] = json!(s.revision);
    result["preview"] = preview_json(&s.preview);
    Ok(result)
}

fn assign(state: &mut AppState, s: &EditorSession, a: &Args<'_>) -> R<usize> {
    let system = selected_system(s, a)?;
    let name = s.doc.system_name(system).ok_or("no particle system")?.to_owned();
    super::tools::check_value("effect_name", &name)?;
    let ids: Vec<u32> = state
        .doc
        .selection()
        .objects
        .iter()
        .filter_map(|o| {
            if let hp_editor::ObjRef::Entity(id) = o {
                state
                    .doc
                    .scene()
                    .entity(*id)
                    .filter(|e| e.entity.classname.eq_ignore_ascii_case("info_particle_system"))
                    .map(|_| *id)
            } else {
                None
            }
        })
        .collect();
    if ids.is_empty() {
        return Err("select one or more info_particle_system entities in the map".into());
    }
    edit(state, "Assign particle system", |doc| doc.set_entity_key(&ids, "effect_name", &name))
}

fn update_preview(p: &mut PreviewOptions, a: &Args<'_>) -> R<()> {
    let mut next = p.clone();
    if let Some(v) = a.opt_bool("paused")? {
        next.paused = v;
    }
    if let Some(v) = a.opt_bool("grid")? {
        next.grid = v;
    }
    if let Some(v) = a.opt_f64("speed", 0.05, 4.0)? {
        next.speed = v as f32;
    }
    if let Some(v) = a.opt_f64("yaw", -36000.0, 36000.0)? {
        next.yaw = v as f32;
    }
    if let Some(v) = a.opt_f64("pitch", -85.0, 85.0)? {
        next.pitch = v as f32;
    }
    if let Some(v) = a.opt_f64("zoom", 0.1, 10.0)? {
        next.zoom = v as f32;
    }
    if let Some(v) = a.get("background") {
        let rgb = float_array::<3>(v)?;
        if rgb.iter().any(|v| !(0.0..=1.0).contains(v)) {
            return Err("background channels must be from zero to one".into());
        }
        next.background = rgb;
    }
    if let Some(v) = a.get("points") {
        let values = v
            .as_array()
            .filter(|v| (1..=64).contains(&v.len()))
            .ok_or("points must contain one to 64 control point positions, including control point zero")?;
        next.points = values.iter().map(float_array::<3>).collect::<R<Vec<_>>>()?;
        if next.points.iter().flatten().any(|v| v.abs() > 131072.0) {
            return Err("control points are outside the coordinate range".into());
        }
    }
    if a.opt_bool("restart")?.unwrap_or(false) {
        next.restart = next.restart.wrapping_add(1);
    }
    *p = next;
    Ok(())
}

fn reference_json(r: &Reference) -> Value {
    match r {
        Reference::Element(id) => json!(id),
        Reference::External(s) => json!(s),
    }
}

fn value_json(v: &DmxValue) -> Value {
    let (kind, value) = match v {
        DmxValue::Element(v) => ("element", json!(v)),
        DmxValue::Int(v) => ("int", json!(v)),
        DmxValue::Float(v) => ("float", json!(v)),
        DmxValue::Bool(v) => ("bool", json!(v)),
        DmxValue::Str(v) => ("string", json!(v)),
        DmxValue::Color(v) => ("color", json!(v)),
        DmxValue::Vec2(v) => ("vec2", json!(v)),
        DmxValue::Vec3(v) => ("vec3", json!(v)),
        DmxValue::Vec4(v) => ("vec4", json!(v)),
        DmxValue::Elements(v) => ("elements", json!(v)),
        DmxValue::Ints(v) => ("ints", json!(v)),
        DmxValue::Floats(v) => ("floats", json!(v)),
        DmxValue::Vec3s(v) => ("vec3s", json!(v)),
        DmxValue::Strs(v) => ("strings", json!(v)),
        DmxValue::External(v) => ("external", json!(v)),
        DmxValue::References(v) => ("references", json!(v.iter().map(reference_json).collect::<Vec<_>>())),
        DmxValue::Raw { kind, data } => ("raw", json!({"kind":kind,"data":data})),
        DmxValue::Other => ("other", Value::Null),
    };
    json!({"type":kind,"value":value,"value_json":value.to_string()})
}

fn float_value(value: &Value) -> R<f32> {
    let number = value
        .as_f64()
        .filter(|f| f.is_finite() && f.abs() <= f32::MAX as f64)
        .ok_or("expected a finite 32 bit number")?;
    Ok(number as f32)
}

fn float_array<const N: usize>(value: &Value) -> R<[f32; N]> {
    let values =
        value.as_array().filter(|v| v.len() == N).ok_or_else(|| format!("expected an array of {N} numbers"))?;
    let mut out = [0.0; N];
    for (target, value) in out.iter_mut().zip(values) {
        *target = float_value(value)?;
    }
    Ok(out)
}

fn reference_value(value: &Value) -> R<Reference> {
    if value.is_null() {
        return Ok(Reference::Element(None));
    }
    if let Some(s) = value.as_str() {
        if s.contains('\0') {
            return Err("external reference contains NUL".into());
        }
        return Ok(Reference::External(s.into()));
    }
    let id = value
        .as_u64()
        .filter(|n| *n <= 1000000)
        .ok_or("expected an element id, null or an external reference string")?;
    Ok(Reference::Element(Some(id as usize)))
}

fn decode_value(kind: &str, value: &Value) -> R<DmxValue> {
    let list = || {
        value
            .as_array()
            .filter(|a| a.len() <= 65536)
            .ok_or_else(|| "expected an array of at most 65536 values".to_owned())
    };
    let text = || {
        value
            .as_str()
            .filter(|s| !s.contains('\0'))
            .map(str::to_owned)
            .ok_or_else(|| "expected a string without NUL characters".to_owned())
    };
    let int = |v: &Value| {
        v.as_i64().and_then(|n| i32::try_from(n).ok()).ok_or_else(|| "expected a 32 bit integer".to_owned())
    };
    let byte = |v: &Value| {
        v.as_u64().and_then(|n| u8::try_from(n).ok()).ok_or_else(|| "expected a byte from 0 to 255".to_owned())
    };
    Ok(match kind {
        "element" => match reference_value(value)? {
            Reference::Element(v) => DmxValue::Element(v),
            Reference::External(s) => DmxValue::External(s),
        },
        "external" => DmxValue::External(text()?),
        "int" => DmxValue::Int(int(value)?),
        "float" => DmxValue::Float(float_value(value)?),
        "bool" => DmxValue::Bool(value.as_bool().ok_or("expected true or false")?),
        "string" => DmxValue::Str(text()?),
        "vec2" => DmxValue::Vec2(float_array(value)?),
        "vec3" => DmxValue::Vec3(float_array(value)?),
        "vec4" => DmxValue::Vec4(float_array(value)?),
        "color" => {
            let values = list()?;
            if values.len() != 4 {
                return Err("color must contain four byte channels".into());
            }
            let mut out = [0; 4];
            for (to, v) in out.iter_mut().zip(values) {
                *to = byte(v)?;
            }
            DmxValue::Color(out)
        }
        "elements" => DmxValue::Elements(
            list()?
                .iter()
                .map(|v| match reference_value(v)? {
                    Reference::Element(id) => Ok(id),
                    Reference::External(_) => Err("use type references for external reference arrays".into()),
                })
                .collect::<R<Vec<_>>>()?,
        ),
        "references" => DmxValue::References(list()?.iter().map(reference_value).collect::<R<Vec<_>>>()?),
        "ints" => DmxValue::Ints(list()?.iter().map(int).collect::<R<Vec<_>>>()?),
        "floats" => DmxValue::Floats(list()?.iter().map(float_value).collect::<R<Vec<_>>>()?),
        "vec3s" => DmxValue::Vec3s(list()?.iter().map(float_array).collect::<R<Vec<_>>>()?),
        "strings" => DmxValue::Strs(
            list()?
                .iter()
                .map(|v| {
                    v.as_str()
                        .filter(|s| !s.contains('\0'))
                        .map(str::to_owned)
                        .ok_or_else(|| "expected strings without NUL characters".into())
                })
                .collect::<R<Vec<_>>>()?,
        ),
        "raw" => {
            let object = value.as_object().ok_or("raw must be an object containing kind and data")?;
            if object.len() != 2 || !object.contains_key("kind") || !object.contains_key("data") {
                return Err("raw must contain exactly kind and data".into());
            }
            let kind = byte(&object["kind"])?;
            let data = object["data"]
                .as_array()
                .filter(|a| a.len() <= 65536)
                .ok_or("raw data must be an array of at most 65536 bytes")?
                .iter()
                .map(byte)
                .collect::<R<Vec<_>>>()?;
            DmxValue::Raw { kind, data }
        }
        _ => return Err("unknown parameter type".into()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(state: &mut AppState, s: &mut EditorSession, value: Value) -> R<Value> {
        let spec = SPECS.iter().find(|s| s.name == "particle_editor").unwrap();
        let args = checked(spec, &value)?;
        validate_editor_arguments(&args)?;
        apply_editor(state, s, &args)
    }

    #[test]
    fn particle_parameter_types_are_lossless_and_checked() {
        let values = [
            DmxValue::Element(Some(2)),
            DmxValue::Element(None),
            DmxValue::Int(i32::MIN),
            DmxValue::Float(0.125),
            DmxValue::Bool(true),
            DmxValue::Str("escaped \" quote".into()),
            DmxValue::Color([1, 2, 3, 255]),
            DmxValue::Vec2([1.0; 2]),
            DmxValue::Vec3([2.0; 3]),
            DmxValue::Vec4([3.0; 4]),
            DmxValue::Elements(vec![Some(1), None]),
            DmxValue::Ints(vec![-1, 2]),
            DmxValue::Floats(vec![0.5, 1.0]),
            DmxValue::Vec3s(vec![[1.0; 3]]),
            DmxValue::Strs(vec!["abc".into()]),
            DmxValue::External("external-guid".into()),
            DmxValue::References(vec![Reference::Element(None), Reference::External("id".into())]),
            DmxValue::Raw { kind: 12, data: vec![0; 12] },
        ];
        for original in values {
            let encoded = value_json(&original);
            assert_eq!(decode_value(encoded["type"].as_str().unwrap(), &encoded["value"]).unwrap(), original);
        }
        for (kind, value) in [
            ("int", json!(2147483648u64)),
            ("float", json!(1e100)),
            ("vec3", json!([1, 2])),
            ("color", json!([1, 2, 3, 256])),
            ("elements", json!(["external"])),
            ("string", json!("nul\u{0}")),
            ("bool", json!(1)),
            ("raw", json!({"kind":256,"data":[]})),
        ] {
            assert!(decode_value(kind, &value).is_err(), "{kind}");
        }
    }

    #[test]
    fn particle_schemas_reject_invalid_controls_and_operations() {
        let mut state = hp_app::testing::test_state();
        for value in [
            json!({"op":"unknown"}),
            json!({"op":"preview","points":[[0,1]]}),
            json!({"op":"preview","background":[1,2,0]}),
            json!({"op":"preview","speed":-1}),
            json!({"op":"preview","speed":0}),
            json!({"op":"preview","speed":4.1}),
            json!({"op":"preview","zoom":0}),
            json!({"op":"preview","zoom":0.05}),
            json!({"op":"preview","pitch":86}),
            json!({"op":"preview","points":[]}),
            json!({"op":"new","unused":true}),
        ] {
            let result = run_tool(&mut state, "particle_editor", &value).unwrap();
            assert!(matches!(result,Outcome::Done(ref answer) if answer.is_error), "{value}: {result:?}");
        }
        assert!(matches!(
            run_tool(
                &mut state,
                "particle_editor",
                &json!({"op":"preview","paused":true,"points":[[1,2,3]],"background":[0.1,0.1,0.1]})
            ),
            Some(Outcome::AppFn(_))
        ));
    }

    #[test]
    fn particle_document_edits_history_and_discard_protection() {
        let mut state = hp_app::testing::test_state();
        let mut session = EditorSession::default();
        apply(&mut state, &mut session, json!({"op":"create","name":"smoke"})).unwrap();
        let system = session.selected.unwrap();
        assert!(session.dirty());
        assert!(apply(&mut state, &mut session, json!({"op":"new"})).is_err());
        apply(
            &mut state,
            &mut session,
            json!({"op":"add_operator","category":"emitters","function":"emit_continuously"}),
        )
        .unwrap();
        let operator = session.selected_element.unwrap();
        assert_eq!(session.doc.element(operator).unwrap().float("emission_rate"), Some(100.0));
        apply(
            &mut state,
            &mut session,
            json!({"op":"set_parameter","key":"emission_rate","type":"float","value_json":"12.5"}),
        )
        .unwrap();
        assert_eq!(session.doc.element(operator).unwrap().float("emission_rate"), Some(12.5));
        apply(&mut state, &mut session, json!({"op":"undo"})).unwrap();
        assert_eq!(session.doc.element(operator).unwrap().float("emission_rate"), Some(100.0));
        apply(&mut state, &mut session, json!({"op":"redo"})).unwrap();
        assert_eq!(session.doc.element(operator).unwrap().float("emission_rate"), Some(12.5));
        apply(&mut state, &mut session, json!({"op":"create","name":"child"})).unwrap();
        let child = session.selected.unwrap();
        apply(&mut state, &mut session, json!({"op":"add_child","system":system,"child":child,"delay":0.3})).unwrap();
        assert!(apply(&mut state, &mut session, json!({"op":"add_child","system":child,"child":system})).is_err());
        assert_eq!(document_json(&session, Some(operator), 0, 100).unwrap()["total"], 2);
        apply(&mut state, &mut session, json!({"op":"new","confirm":true})).unwrap();
        assert!(session.doc.systems().is_empty());
    }

    #[test]
    fn particle_files_save_replace_import_and_export() {
        let base = if Path::new("E:/hp-scratch").is_dir() {
            PathBuf::from("E:/hp-scratch/pcf111")
        } else {
            std::env::temp_dir().join("hp-pcf111")
        };
        let directory = base.join(format!("mcp-files-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("effect.pcf");
        let mut state = hp_app::testing::test_state();
        let mut session = EditorSession::default();
        apply(&mut state, &mut session, json!({"op":"preset","name":"smoke"})).unwrap();
        apply(&mut state, &mut session, json!({"op":"save","path":path})).unwrap();
        assert!(!session.dirty());
        apply(&mut state, &mut session, json!({"op":"rename","name":"saved_smoke"})).unwrap();
        apply(&mut state, &mut session, json!({"op":"save","path":path})).unwrap();
        assert_eq!(Document::parse(&std::fs::read(&path).unwrap()).unwrap().definitions()[0].name, "saved_smoke");
        apply(&mut state, &mut session, json!({"op":"import","file":path})).unwrap();
        assert_eq!(session.doc.systems().len(), 2);
        let exported =
            apply(&mut state, &mut session, json!({"op":"export","path":directory.join("addon"),"name":"effects.pcf"}))
                .unwrap();
        assert!(exported["lua"].as_str().unwrap().contains("game.AddParticles"));
        assert!(apply(&mut state, &mut session, json!({"op":"save","path":"relative.pcf"})).is_err());
        std::fs::remove_dir_all(&directory).unwrap();
    }
}
