use hp_app::AppState;
use hp_editor::{EditError, Id, ObjRef};
use hp_geom::roadside::{AlphaMode, BlendShape, GutterShape, HeightMode, ShoulderFalloff, Sides, Spine};
use hp_mcp::protocol::ToolResult;
use hp_mcp::tools::{ToolKind, ToolSpec};
use hp_tools_create::curve::apply::{Created, Target};
use hp_tools_create::roadside::{self, ApplyError, Kind, RoadsideMeta, Source};
use hp_tools_mesh::disp::roadblend;
use serde_json::{Map, Value, json};

use super::semantic::{boolean, choice, ids, integer, number, object, string};
use super::tools::{Args, Domain, MAX_NAME, Outcome, R, bounds_json, check_class, edit, material_name};

pub(crate) const DOMAIN: Domain = Domain { category: "road details", specs: SPECS, run: run_tool };

const SIDES: [&str; 3] = ["left", "right", "both"];
const FALLOFFS: [&str; 4] = ["smooth", "linear", "ease_in", "ease_out"];
const MODES: [&str; 3] = ["both", "sink", "raise"];
const TEXTURES: [&str; 3] = ["off", "two_way", "four_way"];

const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "create_roadside",
        title: "Sidewalks, curbs and gutters",
        description: "Generate the details beside roads or curves as convex brushes, in one undo step, like the \
            Sidewalk generator and Curb and gutter generator windows of the Tools menu. kind sidewalk builds a \
            raised walk of a width and a height above the road edge with an optional bevelled curb face and \
            cross slope. kind curb builds a curb (height, width, bevel or rounded top) and, with gutter true, a V \
            or U gutter channel beside the road with an optional drain slot every few pieces. Roads are the road \
            tool roads and the curves made by the curve tool or create_curve_brushes (road_ids, default the \
            selected ones). A curve of the Front or Side view gets its details in its own plane, with heights along \
            the depth axis of that view, and a set cannot mix views. Several roads are handled together: a road ending \
            on the side of another one, or crossing it, is a junction, whose mouths stay open and whose corners are \
            rounded with corner_radius (measured at the road edge). The set remembers its roads and numbers, so \
            edit_roadside and the Edit road details command can change it later. The new brushes are selected.",
        kind: ToolKind::Edit,
        schema: create_schema,
    },
    ToolSpec {
        name: "edit_roadside",
        title: "Edit road details",
        description: "Re-open a set made by create_roadside or the generator windows and replace its brushes in one \
            undo step: give the solid_id of any brush of the set (default: the selected one) and only the fields \
            that change. road_ids replaces the roads; leaving every field out rebuilds the set from the roads as \
            they are now, which follows a road that was edited since. The kind cannot change.",
        kind: ToolKind::Edit,
        schema: edit_schema,
    },
    ToolSpec {
        name: "get_roadside",
        title: "Read road details",
        description: "Read the numbers stored on a set made by create_roadside or the generator windows: kind, \
            sides, gap, corner radius, the sidewalk, curb and gutter numbers, the materials, the roads it follows \
            and the ids of all its brushes.",
        kind: ToolKind::Read,
        schema: get_schema,
    },
    ToolSpec {
        name: "blend_terrain_to_road",
        title: "Blend terrain to road",
        description: "Make every displacement within reach of the roads follow them, in one undo step: the vertices \
            under the road and margin units beside it go to the road height (minus sink), and across the shoulder \
            they fade back to the terrain with the falloff and the strength. mode sink only lowers, raise only \
            lifts. texture_blend two_way moves the vertex alpha, four_way the multiblend weights, toward the road \
            texture (road_channel, 0 to 3, or 0 and 1 for two_way) over texture_width, fading to the terrain \
            texture. Neighbouring displacements are sewn afterwards. The roads (road_ids, default the selected \
            roads) must be seen from above: a curve of the Front or Side view is refused with a message.",
        kind: ToolKind::Edit,
        schema: blend_schema,
    },
    ToolSpec {
        name: "preview_terrain_blend",
        title: "Preview terrain blend",
        description: "Count what blend_terrain_to_road would change with the same fields and show the affected area \
            in the 3D view: the new heights as a tinted surface (orange for the height, blue for the texture \
            only) and the road edge and shoulder limits as lines. show false only counts, clear true removes the \
            preview. Nothing in the map changes.",
        kind: ToolKind::Read,
        schema: preview_schema,
    },
];

fn run_tool(state: &mut AppState, name: &str, v: &Value) -> Option<Outcome> {
    let spec = SPECS.iter().find(|s| s.name == name)?;
    let result: R<ToolResult> = match name {
        "create_roadside" => super::semantic::checked(spec, v).and_then(|a| create_roadside(state, &a)),
        "edit_roadside" => super::semantic::checked(spec, v).and_then(|a| edit_roadside(state, &a)),
        "get_roadside" => super::semantic::checked(spec, v).and_then(|a| get_roadside(state, &a)),
        "blend_terrain_to_road" => super::semantic::checked(spec, v).and_then(|a| blend_terrain(state, &a)),
        "preview_terrain_blend" => super::semantic::checked(spec, v).and_then(|a| preview_blend(state, &a)),
        _ => return None,
    };
    Some(result.unwrap_or_else(|e| ToolResult::error(format!("{name}: {e}"))).into())
}

fn roads_prop() -> Value {
    ids(
        "Solid or entity ids of the roads: road tool roads and curves. The terrain tools need roads seen from above. Default: the selected ones.",
    )
}

fn detail_props() -> Map<String, Value> {
    let mut p = Map::new();
    let mut put = |k: &str, v: Value| {
        p.insert(k.to_owned(), v);
    };
    put("road_ids", roads_prop());
    put("sides", choice("Which side of the roads, following their direction (default both).", &SIDES));
    put("gap", number("Distance from the road edge to the first brush (default 0).", 0.0, 512.0));
    put(
        "corner_radius",
        number("Junction corners: radius measured at the road edge, 0 for sharp corners (default 32).", 0.0, 512.0),
    );
    put("sidewalk_width", number("Sidewalk: width in units (default 64).", 1.0, 512.0));
    put("sidewalk_height", number("Sidewalk: height above the road edge (default 6).", 0.0, 256.0));
    put("sidewalk_depth", number("Sidewalk: how far the brush goes under the road edge (default 16).", 1.0, 256.0));
    put(
        "sidewalk_bevel",
        number("Sidewalk: chamfer of the top edge on the road side, the curb face (default 0).", 0.0, 256.0),
    );
    put(
        "sidewalk_slope",
        number(
            "Sidewalk: rise of the top from the road side to the far side, negative drains toward the road (default 0).",
            -512.0,
            512.0,
        ),
    );
    put("curb", boolean("Curb sets: build the curb (default true)."));
    put("curb_width", number("Curb: width (default 8).", 1.0, 512.0));
    put("curb_height", number("Curb: height above the road edge (default 6).", 1.0, 256.0));
    put("curb_depth", number("Curb: how far the brush goes under the road edge (default 8).", 1.0, 256.0));
    put("curb_bevel", number("Curb: size of the chamfer or of the rounding of the top edge (default 0).", 0.0, 256.0));
    put(
        "curb_rounded",
        boolean("Curb: round the top edge with a quarter circle instead of a chamfer (default false)."),
    );
    put("curb_both_edges", boolean("Curb: treat the far top edge like the road side one (default false)."));
    put("gutter", boolean("Curb sets: build a gutter channel beside the road, before the curb (default false)."));
    put("gutter_width", number("Gutter: width (default 16).", 2.0, 512.0));
    put("gutter_depth", number("Gutter: how far the channel sinks under the road edge (default 3).", 0.5, 256.0));
    put("gutter_shape", choice("Gutter: V or U section (default v).", &["v", "u"]));
    put("gutter_floor", number("Gutter: width of the flat bottom of a U (default 6).", 0.0, 512.0));
    put("drain_slot", boolean("Gutter: cut a drain slot into the channel every slot_every pieces (default false)."));
    put("slot_width", number("Drain slot: width across the channel (default 4).", 0.5, 512.0));
    put("slot_depth", number("Drain slot: how far under the channel floor (default 12).", 1.0, 256.0));
    put("slot_every", integer("Drain slot: one slot every this many pieces along the road (default 4).", 1, 1024));
    put("material", string("Sidewalk top, or curb material (default: the current material)."));
    put("face_material", string("Material of the faces turned toward the road, the curb face (default: the same)."));
    put("curb_material", string("Curb material when a sidewalk set has none (default: material)."));
    put("gutter_material", string("Gutter material (default: material)."));
    put("tex_scale", number("Texture scale of the new faces (default: the editor's).", 0.01, 64.0));
    p
}

fn create_schema() -> Value {
    let mut p = detail_props();
    p.insert("kind".to_owned(), choice("What to build.", &["sidewalk", "curb"]));
    p.insert(
        "entity_class".to_owned(),
        string("Put every brush in one brush entity of this class (func_detail...) instead of the world."),
    );
    object(Value::Object(p), &["kind"])
}

fn solid_prop() -> Value {
    integer("A brush of the set (or its brush entity). Default: the selected one.", 1, i64::from(u32::MAX))
}

fn edit_schema() -> Value {
    let mut p = detail_props();
    p.insert("solid_id".to_owned(), solid_prop());
    object(Value::Object(p), &[])
}

fn get_schema() -> Value {
    object(json!({ "solid_id": solid_prop() }), &[])
}

fn terrain_props() -> Map<String, Value> {
    let mut p = Map::new();
    let mut put = |k: &str, v: Value| {
        p.insert(k.to_owned(), v);
    };
    put("road_ids", roads_prop());
    put("shoulder", number("Width of the fade from the road height back to the terrain (default 128).", 0.0, 4096.0));
    put("margin", number("Extra width beside the road kept fully at the road height (default 0).", 0.0, 1024.0));
    put("falloff", choice("Shape of the fade (default smooth).", &FALLOFFS));
    put("strength", number("How far the terrain goes toward the road, 0 to 1 (default 1).", 0.0, 1.0));
    put("sink", number("Put the terrain this many units under the road surface (default 1).", -256.0, 256.0));
    put("mode", choice("both moves up and down (default), sink only lowers, raise only lifts.", &MODES));
    put("texture_blend", choice("Blend the displacement texture toward the road texture (default off).", &TEXTURES));
    put("texture_width", number("Width of the texture fade past the road edge (default 64).", 0.0, 4096.0));
    put("texture_strength", number("How far the texture goes toward the road texture, 0 to 1 (default 1).", 0.0, 1.0));
    put("road_channel", integer("Texture of the road: 0 to 3 for four_way, 0 or 1 for two_way (default 1).", 0, 3));
    p
}

fn blend_schema() -> Value {
    object(Value::Object(terrain_props()), &[])
}

fn preview_schema() -> Value {
    let mut p = terrain_props();
    p.insert("show".to_owned(), boolean("Show the area in the 3D view (default true)."));
    p.insert("clear".to_owned(), boolean("Remove the preview from the 3D view and do nothing else."));
    object(Value::Object(p), &[])
}

fn invalid(e: ApplyError) -> EditError {
    match e {
        ApplyError::Edit(e) => e,
        other => EditError::Invalid(other.to_string()),
    }
}

fn sources_arg(state: &AppState, a: &Args<'_>) -> R<Vec<Source>> {
    let list = if a.get("road_ids").is_some() {
        let mut objs: Vec<ObjRef> = Vec::new();
        for id in a.ids("road_ids")? {
            if state.doc.scene().solid(id).is_some() {
                objs.push(ObjRef::Solid(id));
            }
            if state.doc.scene().entity(id).is_some() {
                objs.push(ObjRef::Entity(id));
            }
        }
        if objs.is_empty() {
            return Err("road_ids names no solid or entity".to_owned());
        }
        roadside::sources_of_objects(&state.doc, &objs)
    } else {
        roadside::sources_of_selection(&state.doc)
    };
    if list.is_empty() {
        return Err("give road_ids of roads or curves, or select them (road tool or curve tool brushes)".to_owned());
    }
    Ok(list)
}

fn apply_detail_args(a: &Args<'_>, m: &mut RoadsideMeta) -> R<()> {
    if let Some(s) = a.opt_enum("sides", &SIDES)? {
        m.sides = Sides::parse(&s).ok_or("unknown side")?;
    }
    let f = |key: &str, lo: f64, hi: f64, to: &mut f64| -> R<()> {
        if let Some(v) = a.opt_f64(key, lo, hi)? {
            *to = v;
        }
        Ok(())
    };
    f("gap", 0.0, 512.0, &mut m.gap)?;
    f("corner_radius", 0.0, 512.0, &mut m.radius)?;
    f("sidewalk_width", 1.0, 512.0, &mut m.sidewalk.width)?;
    f("sidewalk_height", 0.0, 256.0, &mut m.sidewalk.height)?;
    f("sidewalk_depth", 1.0, 256.0, &mut m.sidewalk.depth)?;
    f("sidewalk_bevel", 0.0, 256.0, &mut m.sidewalk.bevel)?;
    f("sidewalk_slope", -512.0, 512.0, &mut m.sidewalk.slope)?;
    f("curb_width", 1.0, 512.0, &mut m.curb.width)?;
    f("curb_height", 1.0, 256.0, &mut m.curb.height)?;
    f("curb_depth", 1.0, 256.0, &mut m.curb.depth)?;
    f("curb_bevel", 0.0, 256.0, &mut m.curb.bevel)?;
    f("gutter_width", 2.0, 512.0, &mut m.gutter.width)?;
    f("gutter_depth", 0.5, 256.0, &mut m.gutter.depth)?;
    f("gutter_floor", 0.0, 512.0, &mut m.gutter.floor_width)?;
    f("slot_width", 0.5, 512.0, &mut m.gutter.slot_width)?;
    f("slot_depth", 1.0, 256.0, &mut m.gutter.slot_depth)?;
    f("tex_scale", 0.01, 64.0, &mut m.tex_scale)?;
    let flag = |key: &str, to: &mut bool| -> R<()> {
        if let Some(v) = a.opt_bool(key)? {
            *to = v;
        }
        Ok(())
    };
    flag("curb", &mut m.use_curb)?;
    flag("curb_rounded", &mut m.curb.rounded)?;
    flag("curb_both_edges", &mut m.curb.both_edges)?;
    flag("gutter", &mut m.use_gutter)?;
    flag("drain_slot", &mut m.gutter.slot)?;
    if let Some(n) = a.opt_u64("slot_every", 1, 1024)? {
        m.gutter.slot_every = n as u32;
    }
    if let Some(s) = a.opt_enum("gutter_shape", &["v", "u"])? {
        m.gutter.shape = GutterShape::parse(&s).ok_or("unknown gutter shape")?;
    }
    for (key, slot) in [
        ("material", &mut m.materials.top),
        ("face_material", &mut m.materials.face),
        ("curb_material", &mut m.materials.curb),
        ("gutter_material", &mut m.materials.gutter),
    ] {
        if let Some(name) = a.opt_str(key, MAX_NAME)? {
            *slot = if name.trim().is_empty() { String::new() } else { material_name(name)? };
        }
    }
    Ok(())
}

fn lightmap_scale(state: &AppState) -> i32 {
    match state.settings.edit.default_lightmap_scale {
        s if s > 0 => s,
        _ => 16,
    }
}

fn meta_json(m: &RoadsideMeta) -> Value {
    json!({
        "kind": m.kind.name(),
        "sides": m.sides.name(),
        "gap": m.gap,
        "corner_radius": m.radius,
        "sidewalk_width": m.sidewalk.width,
        "sidewalk_height": m.sidewalk.height,
        "sidewalk_depth": m.sidewalk.depth,
        "sidewalk_bevel": m.sidewalk.bevel,
        "sidewalk_slope": m.sidewalk.slope,
        "curb": m.use_curb,
        "curb_width": m.curb.width,
        "curb_height": m.curb.height,
        "curb_depth": m.curb.depth,
        "curb_bevel": m.curb.bevel,
        "curb_rounded": m.curb.rounded,
        "curb_both_edges": m.curb.both_edges,
        "gutter": m.use_gutter,
        "gutter_width": m.gutter.width,
        "gutter_depth": m.gutter.depth,
        "gutter_shape": m.gutter.shape.name(),
        "gutter_floor": m.gutter.floor_width,
        "drain_slot": m.gutter.slot,
        "slot_width": m.gutter.slot_width,
        "slot_depth": m.gutter.slot_depth,
        "slot_every": m.gutter.slot_every,
        "material": m.materials.top,
        "face_material": m.materials.face,
        "curb_material": m.materials.curb,
        "gutter_material": m.materials.gutter,
        "tex_scale": m.tex_scale,
        "roads": m.sources.iter().map(Source::label).collect::<Vec<_>>(),
    })
}

fn finish(state: &mut AppState, meta: &RoadsideMeta, made: &Created) -> R<ToolResult> {
    let bounds =
        made.ids.iter().filter_map(|id| state.doc.scene().solid(*id).map(|n| n.bounds)).reduce(|a, b| a.union(&b));
    let mut out = meta_json(meta);
    if let Value::Object(map) = &mut out {
        map.insert("solid_ids".to_owned(), json!(made.ids));
        map.insert("entity_id".to_owned(), json!(made.entity));
        map.insert("count".to_owned(), json!(made.ids.len()));
        map.insert("bounds".to_owned(), bounds_json(bounds));
    }
    Ok(ToolResult::json(out))
}

fn create_roadside(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let kind = a.opt_enum("kind", &["sidewalk", "curb"])?.and_then(|k| Kind::parse(&k)).ok_or("kind is required")?;
    let mut meta = RoadsideMeta { kind, sources: sources_arg(state, a)?, ..RoadsideMeta::default() };
    let scale = state.settings.edit.default_texture_scale;
    meta.tex_scale = if scale.is_finite() && scale.abs() > 1e-6 { scale } else { hp_geom::DEFAULT_TEXTURE_SCALE };
    meta.materials.top.clone_from(&state.current_material);
    apply_detail_args(a, &mut meta)?;
    let class = a.opt_str("entity_class", MAX_NAME)?.map(|c| c.trim().to_owned());
    if let Some(c) = &class {
        check_class(c)?;
    }
    let lightmap = lightmap_scale(state);
    let made = edit(state, "create_roadside", |doc| {
        let target = match &class {
            Some(c) => Target::NewEntity { class: c, keyvalues: &[] },
            None => Target::World,
        };
        roadside::create(doc, &meta, lightmap, target).map_err(invalid)
    })?;
    finish(state, &meta, &made)
}

fn piece_arg(state: &AppState, a: &Args<'_>) -> R<Id> {
    if let Some(id) = a.opt_u64("solid_id", 1, u64::from(u32::MAX))? {
        return roadside::piece_of(&state.doc, id as u32)
            .ok_or_else(|| format!("{id} is not a brush made by create_roadside or the road details generators"));
    }
    state.doc.selected_roadside_piece().ok_or_else(|| {
        "give a solid_id of a road details brush, or select one made by create_roadside or the generator windows"
            .to_owned()
    })
}

fn get_roadside(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let piece = piece_arg(state, a)?;
    let (record, meta) = roadside::read(&state.doc, piece).ok_or("this set has no readable road details data")?;
    let mut out = meta_json(&meta);
    if let Value::Object(map) = &mut out {
        map.insert("solid_ids".to_owned(), json!(record.ids));
        let entity = match record.owner {
            hp_editor::Owner::Entity(e) => Some(e),
            hp_editor::Owner::World => None,
        };
        map.insert("entity_id".to_owned(), json!(entity));
    }
    Ok(ToolResult::json(out))
}

fn edit_roadside(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let piece = piece_arg(state, a)?;
    let (record, mut meta) = roadside::read(&state.doc, piece).ok_or("this set has no readable road details data")?;
    if a.get("road_ids").is_some() {
        meta.sources = sources_arg(state, a)?;
    }
    apply_detail_args(a, &mut meta)?;
    let lightmap = roadside::lightmap_of(&state.doc, &record);
    let made = edit(state, "edit_roadside", |doc| roadside::replace(doc, &record, &meta, lightmap).map_err(invalid))?;
    finish(state, &meta, &made)
}

fn shape_args(a: &Args<'_>) -> R<BlendShape> {
    let mut s = BlendShape::default();
    let f = |key: &str, lo: f64, hi: f64, to: &mut f64| -> R<()> {
        if let Some(v) = a.opt_f64(key, lo, hi)? {
            *to = v;
        }
        Ok(())
    };
    f("shoulder", 0.0, 4096.0, &mut s.shoulder)?;
    f("margin", 0.0, 1024.0, &mut s.margin)?;
    f("strength", 0.0, 1.0, &mut s.strength)?;
    f("sink", -256.0, 256.0, &mut s.sink)?;
    f("texture_width", 0.0, 4096.0, &mut s.alpha_shoulder)?;
    f("texture_strength", 0.0, 1.0, &mut s.alpha_strength)?;
    if let Some(v) = a.opt_enum("falloff", &FALLOFFS)? {
        s.falloff = ShoulderFalloff::parse(&v).ok_or("unknown falloff")?;
    }
    if let Some(v) = a.opt_enum("mode", &MODES)? {
        s.mode = HeightMode::parse(&v).ok_or("unknown mode")?;
    }
    if let Some(v) = a.opt_enum("texture_blend", &TEXTURES)? {
        s.alpha = AlphaMode::parse(&v).ok_or("unknown texture blend")?;
    }
    if let Some(n) = a.opt_u64("road_channel", 0, 3)? {
        s.road_channel = n as usize;
    }
    Ok(s)
}

fn spines_arg(state: &AppState, a: &Args<'_>) -> R<Vec<Spine>> {
    let sources = sources_arg(state, a)?;
    roadside::resolve_all(&state.doc, &sources).map_err(|e| e.to_string())
}

fn blend_terrain(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let shape = shape_args(a)?;
    let spines = spines_arg(state, a)?;
    let report = edit(state, "blend_terrain_to_road", |doc| roadblend::blend_terrain(doc, &spines, &shape))?;
    state.runtime.roadside.clear_preview();
    Ok(ToolResult::json(json!({
        "roads": spines.len(),
        "displacements": report.faces,
        "displacements_changed": report.faces_changed,
        "vertices_moved": report.vertices_moved,
        "vertices_blended": report.vertices_blended,
    })))
}

fn preview_blend(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    if a.opt_bool("clear")?.unwrap_or(false) {
        state.runtime.roadside.clear_preview();
        return Ok(ToolResult::json(json!({ "cleared": true })));
    }
    let shape = shape_args(a)?;
    let spines = spines_arg(state, a)?;
    let preview = roadblend::preview(&state.doc, &spines, &shape).map_err(|e| e.to_string())?;
    let out = json!({
        "roads": spines.len(),
        "displacements": preview.faces,
        "vertices": preview.vertices,
        "triangles": preview.tris.len(),
        "truncated": preview.truncated,
        "shown": a.opt_bool("show")?.unwrap_or(true),
    });
    if a.opt_bool("show")?.unwrap_or(true) {
        state.runtime.roadside.set_preview(preview);
    }
    Ok(ToolResult::json(out))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn road(s: &mut hp_app::AppState, points: serde_json::Value) -> u32 {
        let r = ok(
            s,
            "create_curve_brushes",
            json!({ "view": "top", "points": points, "thickness": 64, "height": 8, "base": 0, "subdivisions": 8, "snap": false }),
        );
        r["solid_ids"][0].as_u64().unwrap() as u32
    }

    #[test]
    fn a_sidewalk_is_built_read_and_edited_through_the_tools() {
        let mut s = hp_app::testing::test_state();
        let road_id = road(&mut s, json!([[0, 0], [400, 0]]));
        let steps = s.doc.history_len().0;
        let r = ok(
            &mut s,
            "create_roadside",
            json!({ "kind": "sidewalk", "road_ids": [road_id], "sides": "left", "sidewalk_width": 48, "material": "dev/walk", "face_material": "dev/face" }),
        );
        assert_eq!(r["count"], 8);
        assert_eq!(s.doc.history_len().0, steps + 1);
        let piece = r["solid_ids"][0].as_u64().unwrap();
        let g = ok(&mut s, "get_roadside", json!({ "solid_id": piece }));
        assert_eq!(
            (g["sides"].as_str(), g["sidewalk_width"].as_f64(), g["material"].as_str()),
            (Some("left"), Some(48.0), Some("dev/walk"))
        );
        let e = ok(&mut s, "edit_roadside", json!({ "solid_id": piece, "sides": "both", "sidewalk_height": 10 }));
        assert_eq!(e["count"], 16);
        assert_eq!(e["sidewalk_width"], 48.0);
        assert_eq!(s.doc.history_len().0, steps + 2);
        assert!(s.doc.undo());
        assert_eq!(ok(&mut s, "get_roadside", json!({ "solid_id": piece }))["sides"], "left");
    }

    #[test]
    fn curb_and_gutter_options_reach_the_set() {
        let mut s = hp_app::testing::test_state();
        let road_id = road(&mut s, json!([[0, 0], [400, 0]]));
        let r = ok(
            &mut s,
            "create_roadside",
            json!({ "kind": "curb", "road_ids": [road_id], "sides": "right", "gutter": true, "gutter_shape": "u", "drain_slot": true, "slot_every": 2, "curb_rounded": true, "curb_bevel": 2, "entity_class": "func_detail" }),
        );
        assert!(r["count"].as_u64().unwrap() > 16);
        assert!(r["entity_id"].as_u64().is_some());
        assert_eq!(r["gutter_shape"], "u");
        err(&mut s, "create_roadside", json!({ "kind": "curb", "road_ids": [road_id], "curb": false }));
    }

    #[test]
    fn bad_input_is_refused_cleanly() {
        let mut s = hp_app::testing::test_state();
        err(&mut s, "create_roadside", json!({ "kind": "sidewalk" }));
        err(&mut s, "create_roadside", json!({ "kind": "wall", "road_ids": [3] }));
        err(&mut s, "create_roadside", json!({ "kind": "sidewalk", "road_ids": [3] }));
        err(&mut s, "edit_roadside", json!({}));
        err(&mut s, "get_roadside", json!({ "solid_id": 3 }));
        let road_id = road(&mut s, json!([[0, 0], [400, 0]]));
        err(&mut s, "create_roadside", json!({ "kind": "sidewalk", "road_ids": [road_id], "sidewalk_width": 0 }));
        err(&mut s, "create_roadside", json!({ "kind": "sidewalk", "road_ids": [road_id], "sidewalk_bevel": 200 }));
    }

    #[test]
    fn the_terrain_blend_tools_validate_and_preview() {
        let mut s = hp_app::testing::test_state();
        let road_id = road(&mut s, json!([[-100, 0], [100, 0]]));
        err(&mut s, "blend_terrain_to_road", json!({ "road_ids": [road_id], "strength": 2 }));
        err(
            &mut s,
            "blend_terrain_to_road",
            json!({ "road_ids": [road_id], "texture_blend": "two_way", "road_channel": 3 }),
        );
        err(&mut s, "blend_terrain_to_road", json!({ "road_ids": [road_id] }));
        let p = ok(&mut s, "preview_terrain_blend", json!({ "road_ids": [road_id], "show": false }));
        assert_eq!(p["displacements"], 0);
        assert!(s.runtime.roadside.preview.is_empty());
        ok(&mut s, "preview_terrain_blend", json!({ "road_ids": [road_id] }));
        assert!(!s.runtime.roadside.preview.lines.is_empty());
        ok(&mut s, "preview_terrain_blend", json!({ "clear": true }));
        assert!(s.runtime.roadside.preview.is_empty());
    }

    #[test]
    fn a_front_curve_gets_details_and_crossings_work_but_terrain_refuses_the_front() {
        let mut s = hp_app::testing::test_state();
        let r = ok(
            &mut s,
            "create_curve_brushes",
            json!({ "view": "front", "points": [[0, 0], [400, 0]], "thickness": 64, "height": 16, "base": 0, "subdivisions": 8, "snap": false }),
        );
        let front = r["solid_ids"][0].as_u64().unwrap() as u32;
        let made = ok(&mut s, "create_roadside", json!({ "kind": "sidewalk", "road_ids": [front] }));
        assert_eq!(made["count"], 16);
        let msg = err(&mut s, "blend_terrain_to_road", json!({ "road_ids": [front] }));
        assert!(msg.contains("from above"), "{msg}");
        let a = road(&mut s, json!([[-300, 0], [300, 0]]));
        let b = road(&mut s, json!([[0, -300], [0, 300]]));
        let x = ok(&mut s, "create_roadside", json!({ "kind": "curb", "road_ids": [a, b], "corner_radius": 40 }));
        assert!(x["count"].as_u64().unwrap() > 32);
    }
}
