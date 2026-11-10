use std::collections::HashMap;

use glam::{DMat4, DQuat, DVec3};
use hp_app::AppState;
use hp_editor::{AlignMode, ClipKeep, FaceEdit, FaceRef, ObjRef, Owner, ReplaceScope, SelectOp};
use hp_mcp::protocol::ToolResult;
use serde_json::{Value, json};

use super::semantic::*;
use super::tools::{Args, MAX_NAME, R, bounds_json, check_class, edit, material_name, obj_json, v3};
use super::tools_curve::{create_curve_brushes, edit_curve_brushes, get_curve_brushes};

domain! { "editing";
    duplicate_objects: Edit, "Duplicate objects",
        "Duplicate objects (groups they fully contain too) with new ids, moved by an offset (default: one grid step \
        on X and Y). The copies are selected.",
        { "ids": target_ids(), "solid_ids": target_solid_ids(), "offset": vec3("Offset [x, y, z] of the copies.") }, [];
    array_duplicate: Edit, "Array of copies",
        "Make several copies of objects. Transform mode (offset, rotate, scale): each copy is moved, rotated and \
        scaled from the previous one, copy N gets N times the offset and the rotation. Grid mode (axes or counts): \
        a grid of copies along the X, Y and Z axes of the active grid, with a gap between the bounding boxes; the \
        step of each axis is rounded up to the grid and positions are aligned to it (rotated and custom grids \
        included) unless snap is false. One undo step.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "axes": strings("Grid mode: axes to duplicate along, any of x, y, z.", 3),
            "counts": json!({ "type": "array", "items": { "type": "integer", "minimum": 0, "maximum": 1024 }, "minItems": 3, "maxItems": 3, "description": "Grid mode: copies per axis [nx, ny, nz], 0 turns an axis off." }),
            "gap": number("Grid mode: space between the bounding boxes on every axis (default 0).", -131072.0, 131072.0),
            "gaps": vec3("Grid mode: space per axis [x, y, z], overrides gap."),
            "snap": boolean("Grid mode: align steps and positions to the active grid (default: the snap setting)."),
            "count": integer("Number of copies (transform mode: 1 to 256; grid mode: copies per axis with axes, 1 to 1024).", 1, 1024),
            "offset": vec3("Offset between two copies [x, y, z]."),
            "rotate": vec3("Rotation between two copies, degrees around X, Y and Z."),
            "scale": number("Scale between two copies (1: same size).", 0.01, 100.0),
            "pivot": vec3("Pivot of the rotation and scale (default: center of the objects)."),
        }, [];
    transform_objects: Edit, "Transform objects",
        "Move, rotate (degrees around X, then Y, then Z) and scale objects around a pivot (default: the center of \
        their bounds), in one step. Entities turn with their angles.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "translate": vec3("Offset [x, y, z]."),
            "rotate": vec3("Degrees around X, Y and Z."),
            "scale": vec3("Scale factors [x, y, z] (0.001 to 1000, use flip_objects to mirror)."),
            "pivot": vec3("Pivot [x, y, z]."),
        }, [];
    resize_objects: Edit, "Resize objects to a box",
        "Scale objects so that their bounds become the box min..max (Hammer's scale handles).",
        { "ids": target_ids(), "solid_ids": target_solid_ids(), "min": vec3("New minimum corner."), "max": vec3("New maximum corner.") }, ["min", "max"];
    flip_objects: Edit, "Flip objects",
        "Mirror objects along an axis, around a center (default: the center of their bounds).",
        { "ids": target_ids(), "solid_ids": target_solid_ids(), "axis": axis("Axis to mirror along."), "center": vec3("Mirror center.") }, ["axis"];
    snap_objects_to_grid: Edit, "Snap to grid",
        "Move objects so the minimum corner of their bounds is on the grid (default: the editor's grid), together \
        or each one on its own.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "grid": number("Grid size in units.", 0.125, 4096.0),
            "individually": boolean("Snap each object on its own (default false)."),
        }, [];
    align_objects: Edit, "Align objects",
        "Line up the minimum, maximum or center of objects on one axis with a reference: another object \
        (reference_id), a value, or the bounds of all of them.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "axis": axis("Axis."),
            "mode": choice("What lines up.", &["min", "max", "center"]),
            "reference_id": integer("Object (entity or solid id) to align with.", 1, i64::from(u32::MAX)),
            "value": number("Coordinate to align with (instead of reference_id).", -131072.0, 131072.0),
            "individually": boolean("Each object on its own (default true)."),
        }, ["axis", "mode"];
    drop_to_surface: Edit, "Drop to surface",
        "Move objects down (or along a direction) until they touch the surface below them, optionally turned to \
        the surface normal. Objects with nothing below stay.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "direction": vec3("Direction (default [0, 0, -1], [0, 0, 1] for the ceiling)."),
            "align": boolean("Turn objects to the surface normal (default false)."),
        }, [];
    clip_brushes: Destructive, "Clip brushes",
        "Cut solids with a plane (the clip tool, Shift+X, in 2D or 3D) through three points (clockwise seen from \
        the front side gives the outward normal like VMF planes), a point and a normal, or a normal and a \
        distance. The front is the side the normal points to. Keep the front, the back or both pieces. Brush \
        entity solids keep their entity, textures are kept, a cut that would leave a piece thinner than 0.25 unit \
        leaves the solid alone, and the whole clip is one undo step. Answers the ids of the resulting solids.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "points": { "type": "array", "items": vec3("Point."), "minItems": 3, "maxItems": 3, "description": "Three points of the plane." },
            "point": vec3("A point of the plane (with normal)."),
            "normal": vec3("Plane normal (with point or distance)."),
            "distance": number("Plane distance from the origin along the normal.", -262144.0, 262144.0),
            "keep": choice("Pieces kept (default both).", &["front", "back", "both"]),
            "cap_material": string("Material of the cut faces (default: the closest face's)."),
        }, [];
    carve: Destructive, "Carve",
        "Carve (subtract) the carving solids from every solid they touch (Hammer's Carve). The carvers stay.",
        { "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    extrude_face: Destructive, "Carve or extrude a face outline",
        "Outline a shape on a brush face, then dig it into the brush or pull it out of the face (the extrude / carve \
        tool, Shift+I). points are world positions, projected on the face plane: any outline that does not cross \
        itself, concave too (it is split into convex pieces). carve replaces the brush by the convex brushes left \
        around the hole: a recess of depth units, or an opening with through. extrude adds the prism on the face as \
        new brushes of the same owner, or grows the brush itself when the result stays convex (an outline covering \
        the whole face). The kept faces keep their textures and alignment; the new faces get the clicked face's \
        texture (new_faces face), the current material (new_faces current) or material. One undo step. Answers the \
        ids of the brushes made or changed (the brush keeps its id when it is carved or grown).",
        {
            "solid": integer("Solid id.", 1, i64::from(u32::MAX)),
            "side": integer("Side index of the face in the solid (get_faces).", 0, 4095),
            "points": { "type": "array", "items": vec3("Point [x, y, z] on the face."), "minItems": 3, "maxItems": 128, "description": "Outline points, in order around the shape." },
            "mode": choice("carve (default): dig into the brush; extrude: pull out of the face.", &["carve", "extrude"]),
            "depth": number("Depth in units (default 16).", 0.125, 16384.0),
            "through": boolean("carve only: cut the whole thickness of the brush (default false)."),
            "new_faces": choice("Texture of the new faces (default face).", &["face", "current"]),
            "material": string("Material of the new faces (instead of new_faces)."),
        }, ["solid", "side", "points"];
 inset_face: Destructive, "Inset faces",
 "Inset brush faces (the Tools > Inset face command and the interactive inset, the I key in the 3D view): the edges of the face move inward by distance units and the brush is split into a frame of convex brushes around the face plus the inner block, so every piece stays a valid convex brush. The frame is made of one quad per edge of the outline (a triangle gives 3, a quad 4, an n-gon n). Several faces make a region inset by default: faces on the same plane that share an edge (the tops of side by side brushes, a floor made of several brushes) are inset together, with one shared outline around their union and one inner block, like Blender's inset without individual; the outline may be concave (an L shape) but must have no hole and no pinch point. individual: true insets every face on its own instead (Blender's individual mode, the I key again in the interactive inset). Faces that are not coplanar and touching (the top and the front of one brush, faces far apart) are always inset on their own. The original textures stay on the visible faces and the new faces get the texture of the inset face (or material). depth raises (positive) or sinks (negative) the inner block in the same step, like Blender I then E. Convex brushes only, displacement faces are refused, and a distance larger than the region allows is refused. faces defaults to the selected faces, else the faces of ids. snap rounds distance and depth to the active grid (size, custom grid and rotation, sizes clamped to 1 to 1024) and puts the new top plane on a grid plane. dry_run answers the number of brushes without changing the map. One undo step; answers the ids of the brushes made or changed.",
        {
            "faces": faces("Faces [{solid, side}] (default: the selected faces)."),
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "distance": number("Inset distance in units (default 16).", 0.125, 16384.0),
            "depth": number("Raise (positive) or sink (negative) the inner block by this many units (default 0).", -16384.0, 16384.0),
            "individual": boolean("Inset every face on its own instead of one shared outline per touching coplanar group (default false)."),
            "snap": boolean("Round distance and depth to the active grid (default false)."),
            "dry_run": boolean("Only answer how many brushes the inset would make, or why it fails (default false)."),
            "material": string("Material of the new faces (default: the inset face's)."),
        }, [];
    extrude_faces: Destructive, "Extrude faces into new brushes or carve them inward",
        "Extrude brush faces outward into new brushes (the interactive extrude, the E key in the 3D view): each face becomes the base of a new convex brush distance units high along the face normal, textured like the face (or material), never merged into the original brush. Several faces make several brushes in one undo step, each pulled along its own normal. faces defaults to the selected faces, else the faces of ids. snap puts the new top plane of every brush on a plane of the active grid (grid size, custom grid and rotation honored, size clamped to 1 to 1024); alt_grid does the same on the alternate grid, which is the Alt key of the interactive extrude: an axis aligned grid at the origin of half the grid size (double when the grid size is 1), ignoring rotation and custom grids. dry_run answers the number of brushes without changing the map. A negative distance carves instead: the slab of material of that depth under each face is cut out of its own brush (the face moves inward, the brush stays one valid convex brush, the answer lists that brush id), in the same single undo step, with the same snap, alt_grid and material behavior; it fails when the carve would go through the whole brush. To carve a pocket or extrude only part of a face use extrude_face. Answers the ids of the new or carved brushes and the distance used for each face.",
        {
            "faces": faces("Faces [{solid, side}] (default: the selected faces)."),
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "distance": number("Height of the new brushes in units, or minus the depth to carve inward when negative (default 16; magnitude at least 0.125).", -16384.0, 16384.0),
            "snap": boolean("Align the new top planes to the active grid (default false)."),
            "alt_grid": boolean("Align them to the alternate grid instead (the Alt key, default false)."),
            "material": string("Material of the new faces (default: the face's)."),
            "dry_run": boolean("Only answer how many brushes would be made, or why it fails (default false)."),
        }, [];
    get_brush_edges: Read, "Brush edges",
        "The edges of a solid as numbered [a, b] point pairs (the edge indices bevel_edges takes), with their length.",
        { "solid": integer("Solid id.", 1, i64::from(u32::MAX)) }, ["solid"];
    bevel_edges: Destructive, "Bevel edges",
        "Round or chamfer brush edges (the bevel tool, Shift+J, or Ctrl+B on an edge in the 3D view). Each edge is \
        an index from get_brush_edges or two points [[x, y, z], [x, y, z]] at its ends, on solid; several edges \
        (up to 64, on one or several solids as {solid, edge} or {solid, a, b} objects) get the same radius. The \
        outward bevel cuts the edge with segments planes and keeps one convex brush (segments 1: a flat \
        chamfer). inverse makes it concave: it carves a cove into the edge (the brush is split into convex \
        pieces), or, when the edge meets another brush at an inner corner (a doorway top, a wall on a floor), it \
        fills that corner with a concave round made of new brushes in the same owner (two corners of a doorway \
        make an arch). The radius is limited to what the faces around the edge allow and segments shorter than \
        half a unit are merged; the answer gives the radius and segments used. The kept faces keep their textures \
        and alignment; the rounded faces get the texture of a face next to the edge wrapped around the whole round \
        (new_faces face), the current material (new_faces current) or material. One undo step, the result is \
        selected. dry_run answers the radius, segments, largest radius and kinds without changing the map.",
        {
            "solid": integer("Solid id of the edges given as indices or points.", 1, i64::from(u32::MAX)),
            "edge": { "description": "One edge: an index from get_brush_edges or [[x, y, z], [x, y, z]]." },
            "edges": { "type": "array", "maxItems": 64, "description": "Edges: indices, [[x, y, z], [x, y, z]] pairs, or {solid, edge} / {solid, a, b} objects." },
            "radius": number("Radius in units (default 16).", 0.25, 16384.0),
            "segments": integer("Faces of the rounding, 1 (chamfer) to 32 (default 4).", 1, 32),
            "inverse": boolean("Concave bevel: cove or inner corner fill (default false)."),
            "new_faces": choice("Texture of the rounded faces (default face).", &["face", "current"]),
            "material": string("Material of the rounded faces (instead of new_faces)."),
            "expand": choice("Bevel the whole edge loop or edge ring of every given edge (see select_edge_loop), across the brushes sharing it; edges with no corner to bevel are skipped (default none).", &["none", "loop", "ring"]),
            "dry_run": boolean("Only answer what the bevel would do (default false)."),
        }, [];
    hollow: Destructive, "Make hollow",
        "Turn solids into rooms with walls of a thickness (negative: walls outside). The walls of each solid are \
        grouped and selected.",
        { "ids": target_ids(), "solid_ids": target_solid_ids(), "thickness": number("Wall thickness (default 16).", -1024.0, 1024.0) }, [];
    merge_brushes: Destructive, "Merge brushes",
        "Merge solids into one. convex_hull false (default) merges only when their union is convex and keeps every \
        texture; true takes their convex hull even when it adds volume.",
        { "ids": target_ids(), "solid_ids": target_solid_ids(), "convex_hull": boolean("Merge into the convex hull.") }, [];
    intersect_brushes: Destructive, "Intersect brushes",
        "Keep only the volume shared by all the given solids (boolean intersection). The first solid keeps its id, owner and editor data and becomes the shared volume; the others are deleted. Faces keep their textures and the new cut faces take the texture of the face that cut them. The solids must all be world solids or belong to the same entity, and must overlap.",
        { "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    union_brushes: Destructive, "Union of brushes",
        "Join solids into the fewest convex solids covering exactly their union (boolean union). Overlapping solids are cut so they no longer overlap, then neighbours are merged wherever the result stays convex, so an L shape stays two solids. Each solid keeps its id when it still has a piece; solids fully inside others are deleted. Fails when nothing can be joined. Answers the solid ids left.",
        { "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    get_brush_vertices: Read, "Brush vertices",
        "The vertices of a solid and its faces as lists of vertex indices (for move_brush_vertices).",
        { "solid": integer("Solid id.", 1, i64::from(u32::MAX)) }, ["solid"];
    move_brush_vertices: Edit, "Move brush vertices",
        "Move some vertices of a solid by an offset (vertex tool). The solid must stay convex; textures follow \
        with uv_lock.",
        {
            "solid": integer("Solid id.", 1, i64::from(u32::MAX)),
            "vertices": { "type": "array", "items": { "type": "integer", "minimum": 0 }, "maxItems": 4096, "description": "Vertex indices (get_brush_vertices)." },
            "offset": vec3("Offset [x, y, z]."),
            "uv_lock": boolean("Textures follow the vertices (default true)."),
        }, ["solid", "vertices", "offset"];
    set_brush_vertices: Edit, "Set brush vertices",
        "Rebuild a solid as the convex hull of new vertex positions (textures from the closest original faces).",
        {
            "solid": integer("Solid id.", 1, i64::from(u32::MAX)),
            "points": { "type": "array", "items": vec3("Vertex."), "minItems": 4, "maxItems": 4096, "description": "Vertices." },
        }, ["solid", "points"];
    create_brush_from_points: Edit, "Brush from points",
        "Create a convex solid, the hull of points, with a material (world brush or brush entity class).",
        {
            "points": { "type": "array", "items": vec3("Vertex."), "minItems": 4, "maxItems": 4096, "description": "Vertices." },
            "material": string("Material (default: the current material)."),
            "entity_class": string("Brush entity class (func_detail...), default the world."),
        }, ["points"];
    create_primitive: Edit, "Create a primitive",
        "Create a block, wedge, cylinder, spike, sphere, arch, torus, stairs, spiral_stairs, ramp, pillar or doorway in a \
        box with every option of the block tool (sides, arch wall width, arc, start angle, add height, torus ring \
        width, tube sides, rotation, spiral rise, stair_steps, stair_step_height, stair_open, spiral_inner, \
        spiral_angle, spiral_start, ramp_angle, pillar_base, pillar_capital, pillar_inset, door_width, door_height, \
        door_thickness, door_arch, door_arch_radius, door_arch_segments, direction (stairs, spiral_stairs and ramp rise toward +y of the box by default), up axis like a primitive drawn in the front or side view, texture scale, brush entity class). With \
        depth_from_selection, the box's extent along that axis comes from the selection like a box drawn in a 2D \
        view (the selection bounds, else the last created object, else 64 units on top). The tool options of the \
        block tool itself (its arch preview window...) are set with set_tool_options.",
        {
            "shape": choice("Shape.", &["block", "wedge", "cylinder", "spike", "sphere", "arch", "torus", "stairs", "spiral_stairs", "ramp", "pillar", "doorway"]),
            "min": vec3("Box minimum."), "max": vec3("Box maximum."),
            "material": string("Material (default: the current material)."),
            "sides": integer("Sides of round shapes.", 3, 128),
            "arch_wall_width": number("Arch wall width.", 1.0, 65536.0),
            "arch_arc": number("Arch arc in degrees.", 1.0, 360.0),
            "arch_start_angle": number("Arch start angle in degrees.", -360.0, 360.0),
            "arch_add_height": number("Arch height added along the arc.", -65536.0, 65536.0),
            "torus_ring_width": number("Torus ring width.", 1.0, 65536.0),
            "torus_tube_sides": integer("Torus tube sides.", 3, 128),
            "torus_arc": number("Torus rotation arc in degrees (over 360 only with torus_rise: a spiral).", 1.0, 3600.0),
            "torus_start_angle": number("Torus rotation start in degrees.", -360.0, 360.0),
            "torus_rise": number("Height gained over the torus arc (spiral ramps, helical pipes).", -16384.0, 16384.0),
            "stair_steps": integer("Stairs and spiral_stairs: number of steps (default 8).", 1, 256),
            "stair_step_height": number("Stairs and spiral_stairs: height of one step, 0 uses stair_steps.", 0.0, 1024.0),
            "stair_open": boolean("Stairs and spiral_stairs: open steps (slabs two steps thick) instead of solid columns."),
            "spiral_inner": number("Spiral stairs: inner radius, 0 for pie steps (default 16).", 0.0, 65536.0),
            "spiral_angle": number("Spiral stairs: total turn in degrees, negative turns the other way (default 270, over 360 needs stair_open).", -3600.0, 3600.0),
            "spiral_start": number("Spiral stairs: start angle in degrees.", -360.0, 360.0),
            "ramp_angle": number("Ramp: slope angle in degrees (default 30).", 1.0, 89.0),
            "pillar_base": number("Pillar: height of the base, 0 for none (default 16).", 0.0, 65536.0),
            "pillar_capital": number("Pillar: height of the capital, 0 for none (default 16).", 0.0, 65536.0),
            "pillar_inset": number("Pillar: how much narrower the shaft is than the base and capital (default 8).", 0.0, 65536.0),
            "door_width": number("Doorway: opening width, 0 is half the wall.", 0.0, 65536.0),
            "door_height": number("Doorway: opening height from the bottom, 0 is three quarters of the wall.", 0.0, 65536.0),
            "door_thickness": number("Doorway: wall thickness centered in the box, 0 fills the box depth.", 0.0, 65536.0),
            "door_arch": boolean("Doorway: give the opening a round top (an arch)."),
            "door_arch_radius": number("Doorway arch: radius, at least half the opening, 0 is exactly half (a semicircle).", 0.0, 65536.0),
            "door_arch_segments": integer("Doorway arch: number of straight pieces of the round top (default 8).", 2, 64),
            "direction": choice("Stairs, spiral_stairs and ramp: side they rise toward, in the shape's frame (default +y; longest is along the longest side of the box; spiral_stairs start there).", &["+y", "-y", "+x", "-x", "longest"]),
            "up": axis("Axis the primitive stands along (default z; x or y like one drawn in the side or front view)."),
            "upside_down": boolean("The primitive grows the other way along its up axis."),
            "tex_scale": number("Texture scale of the new faces (default: the editor's).", 0.01, 64.0),
            "entity_class": string("Make the solids a brush entity of this class (func_detail...) instead of world brushes."),
            "depth_from_selection": axis("Take the box's extent along this axis from the selection (block tool depth)."),
        }, ["min", "max"];
    create_polygon_brush: Edit, "Polygon brush",
        "Create a brush from a convex outline drawn in a 2D view (Hammer++'s polygon tool): the outline points in \
        the view's axes (top: x y, front: y z, side: x z), extruded between two depths along the view's depth axis.",
        {
            "view": choice("2D view of the outline.", &["top", "front", "side"]),
            "points": { "type": "array", "items": vec2("Point [a, b]."), "minItems": 3, "maxItems": 64, "description": "Convex outline, in order." },
            "depth": vec2("Depth range [low, high] along the view axis (z for top, x for front, y for side)."),
            "material": string("Material (default: the current material)."),
        }, ["view", "points", "depth"];
    create_curve_brushes: Edit, "Curve brushes",
        "Build a curved wall, rail or road from box brushes like the curve tool (Shift+U): a smooth spline \
        through the points (in the view's axes: top x y, front y z, side x z) cut in equal lengths into \
        subdivisions brushes, each one a convex 6 sided block of a thickness across the curve and a height along \
        the view's depth axis, with mitred joints (no gap, no overlap). The points snap to the editor's grid \
        unless snap is false. Without base and height the depth comes from the selection like the block tool \
        (else the last created object, else 0 to 64). The brushes are one undo step and are selected. Refused \
        when a bend is too tight for the thickness or a segment would be under 1 unit.",
        {
            "view": choice("2D view the curve is drawn in.", &["top", "front", "side"]),
            "points": { "type": "array", "items": vec2("Point [a, b]."), "minItems": 2, "maxItems": 1024, "description": "Points the curve goes through, in order." },
            "subdivisions": integer("Number of brushes along the curve (default 16).", 2, 4096),
            "thickness": number("Width of the brushes across the curve, centered on it (default 16).", 1.0, 1024.0),
            "height": number("Height of the brushes along the view's depth axis (default: from the selection, else 64).", 1.0, 16384.0),
            "base": number("Start of the brushes along the view's depth axis (z for top, x for front, y for side).", -16384.0, 16384.0),
            "snap": boolean("Snap the points to the editor's grid (default true)."),
            "material": string("Material (default: the current material)."),
            "tex_scale": number("Texture scale of the new faces (default: the editor's).", 0.01, 64.0),
            "entity_class": string("Put every brush in one brush entity of this class (func_detail...) instead of the world."),
            "profile": choice("Cross section: flat (default), curb (a raised kerb on both sides, outside the thickness) or ditch (V shaped channel). Curb makes 3 brushes and ditch 2 per piece of the curve.", &["flat", "curb", "ditch"]),
            "curb_width": number("Profile curb: width of each kerb in units (default 8).", 1.0, 256.0),
            "curb_height": number("Profile curb: height of the kerbs above the road (default 8).", 0.0, 256.0),
            "ditch_depth": number("Profile ditch: depth of the V under the edges (default 8, at least 1 under the height).", 0.0, 512.0),
            "roll_start": number("Banking: roll angle in degrees at the start of the curve (default 0). Positive raises the left side when following the curve.", -60.0, 60.0),
            "roll_end": number("Banking: roll angle in degrees at the end of the curve, interpolated along the curve (default 0).", -60.0, 60.0),
            "auto_bank": number("Automatic banking: degrees of roll for a bend of radius 1000 units, proportional to the curvature, tilting toward the inside of the bend (default 0 is off).", 0.0, 90.0),
            "auto_bank_limit": number("Automatic banking: largest roll in degrees it may add (default 8).", 0.0, 45.0),
        }, ["view", "points"];
    edit_curve_brushes: Edit, "Edit curve brushes",
        "Re-open a curve made by the curve tool or create_curve_brushes and replace its brushes (one undo step): \
        give the solid_id of any brush of the curve (default: the selected curve brush) and only the fields that \
        change; points, subdivisions, thickness, height, base, tex_scale, material and the cross section and \
        banking options keep their stored values when left out. Add or remove points by sending the whole new \
        points list (read the stored one with get_curve_brushes). The new brushes are selected and stay in the \
        same brush entity, if any.",
        {
            "solid_id": integer("A brush of the curve (or its brush entity). Default: the selected curve brush.", 1, i64::from(u32::MAX)),
            "points": { "type": "array", "items": vec2("Point [a, b]."), "minItems": 2, "maxItems": 1024, "description": "New points the curve goes through, in the curve's view axes." },
            "subdivisions": integer("Number of pieces along the curve.", 2, 4096),
            "thickness": number("Width of the road across the curve, centered on it.", 1.0, 1024.0),
            "height": number("Height along the view's depth axis.", 1.0, 16384.0),
            "base": number("Start along the view's depth axis.", -16384.0, 16384.0),
            "snap": boolean("Snap the new points to the editor's grid (default true)."),
            "material": string("Material (default: the material of the existing brushes)."),
            "tex_scale": number("Texture scale of the new faces.", 0.01, 64.0),
            "profile": choice("Cross section: flat (default), curb (a raised kerb on both sides, outside the thickness) or ditch (V shaped channel). Curb makes 3 brushes and ditch 2 per piece of the curve.", &["flat", "curb", "ditch"]),
            "curb_width": number("Profile curb: width of each kerb in units (default 8).", 1.0, 256.0),
            "curb_height": number("Profile curb: height of the kerbs above the road (default 8).", 0.0, 256.0),
            "ditch_depth": number("Profile ditch: depth of the V under the edges (default 8, at least 1 under the height).", 0.0, 512.0),
            "roll_start": number("Banking: roll angle in degrees at the start of the curve (default 0). Positive raises the left side when following the curve.", -60.0, 60.0),
            "roll_end": number("Banking: roll angle in degrees at the end of the curve, interpolated along the curve (default 0).", -60.0, 60.0),
            "auto_bank": number("Automatic banking: degrees of roll for a bend of radius 1000 units, proportional to the curvature, tilting toward the inside of the bend (default 0 is off).", 0.0, 90.0),
            "auto_bank_limit": number("Automatic banking: largest roll in degrees it may add (default 8).", 0.0, 45.0),
        }, [];
    get_curve_brushes: Read, "Read a curve",
        "Read the curve stored on the brushes made by the curve tool or create_curve_brushes: view, points, depth, \
        subdivisions, thickness, cross section and banking options, and the ids of all its brushes.",
        { "solid_id": integer("A brush of the curve (or its brush entity). Default: the selected curve brush.", 1, i64::from(u32::MAX)) }, [];
    snap_brush_vertices: Edit, "Snap brush vertices",
        "Move every vertex of solids to the nearest grid point (default: the editor's grid), fixing off grid brushes \
        (solids that would become invalid are left as they are).",
        { "ids": target_ids(), "solid_ids": target_solid_ids(), "grid": number("Grid size.", 0.125, 4096.0) }, [];
    create_room: Edit, "Create a room",
        "Create a closed room: six walls of a thickness around an inside box (the walls are grouped), with a material \
        for the walls, the floor and the ceiling.",
        {
            "min": vec3("Inside minimum."), "max": vec3("Inside maximum."),
            "thickness": number("Wall thickness (default 16).", 1.0, 1024.0),
            "material": string("Wall material (default: the current material)."),
            "floor_material": string("Floor material (default: the wall material)."),
            "ceiling_material": string("Ceiling material (default: the wall material)."),
        }, ["min", "max"];
    create_stairs: Edit, "Create stairs",
        "Create a flight of stairs filling a box, grouped: a shortcut of create_primitive with shape stairs (same \
        geometry, so step_height, open steps and the other stair options are in create_primitive) that takes the steps \
        and the direction it rises toward.",
        {
            "min": vec3("Box minimum."), "max": vec3("Box maximum."),
            "steps": integer("Number of steps.", 2, 256),
            "direction": choice("Direction the stairs rise toward.", &["+x", "-x", "+y", "-y"]),
            "material": string("Material (default: the current material)."),
        }, ["min", "max", "steps", "direction"];
    make_hint_brushes: Edit, "Make hint brushes",
        "Turn the solids of faces into hint brushes: the faces get tools/toolshint, every other face tools/toolsskip.",
        { "faces": faces("Hint faces [{solid, side}].") }, ["faces"];
    apply_nodraw: Edit, "Apply nodraw",
        "Give faces the nodraw material (tools/toolsnodraw), for example the hidden faces found by \
        find_hidden_faces.",
        { "faces": faces("Faces [{solid, side}].") }, ["faces"];
    tie_to_entity: Edit, "Tie to entity",
        "Tie solids to a new brush entity (Ctrl+T), or each world solid to its own entity (each: true), or to an \
        existing brush entity (entity). A new entity whose FGD class has an origin key (func_tracktrain, \
        func_tanktrain, func_rotating...) gets it at the center of its solids unless keyvalues give one, like \
        Hammer: the game moves and turns the entity around that point.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "entity_class": string("Class of the new entity (default func_detail)."),
            "keyvalues": keyvalues("Keyvalues of the new entity."),
            "each": boolean("One entity per world solid."),
            "entity": integer("Existing brush entity to move the solids into.", 1, i64::from(u32::MAX)),
        }, [];
    move_to_world: Edit, "Move to world",
        "Move the solids of brush entities back to the world; entities left without solids are deleted.",
        { "ids": target_ids(), "solid_ids": target_solid_ids() }, [];
    prop_to_brushes: Destructive, "Convert prop to brushes",
        "Rebuild a prop_static, prop_dynamic or prop_physics as brushes (the reverse of Convert brushes to a static \
        prop): one brush per convex piece of its collision model (the exact brushes of a prop made with Hammer Pro) \
        or of its render mesh, faces textured from the model and nodraw where the model draws nothing. The brushes \
        go into a func_detail or the world and are selected; the prop is removed unless keep_prop.",
        {
            "id": integer("Prop entity id (default: the selected prop).", 1, i64::from(u32::MAX)),
            "target": choice("Where the brushes go (default detail).", &["detail", "world"]),
            "keep_prop": boolean("Keep the prop (default false)."),
            "shape": choice("Where the shape comes from (default auto: the collision model when the model has one, \
                else the render mesh).", &["auto", "collision", "render"]),
        }, [];
    get_faces: Read, "Faces of solids",
        "Every face of solids: material, normal, texture axes, scales, shifts, rotation, lightmap scale, smoothing \
        groups, displacement power and vertices.",
        { "ids": target_ids(), "solid_ids": target_solid_ids(), "vertices": boolean("Include the face vertices (default true).") }, [];
    set_face_texture: Edit, "Edit face textures",
        "Change the texture of faces like the face edit sheet: material, world or face alignment, axes, rotation, \
        scale, shift (absolute or added), lightmap scale, smoothing groups.",
        {
            "faces": faces("Faces (default: the selected faces, else the faces of ids)."),
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "material": string("Material."),
            "align": choice("Reset the axes to world or face alignment.", &["world", "face"]),
            "u_axis": vec3("U axis direction."), "v_axis": vec3("V axis direction."),
            "rotation": number("Absolute rotation in degrees.", -360.0, 360.0),
            "rotate_by": number("Rotation to add in degrees.", -360.0, 360.0),
            "u_scale": number("U scale (units per texel).", -1024.0, 1024.0),
            "v_scale": number("V scale.", -1024.0, 1024.0),
            "scale_by": vec2("Scale multipliers [u, v]."),
            "u_shift": number("U shift in texels.", -65536.0, 65536.0),
            "v_shift": number("V shift in texels.", -65536.0, 65536.0),
            "shift_by": vec2("Shift to add [u, v] in texels."),
            "lightmap_scale": integer("Lightmap scale (luxels per unit, 1 or more).", 1, 1024),
            "smoothing_groups": integer("Smoothing group bit mask.", 0, i64::from(u32::MAX)),
        }, [];
    justify_texture: Edit, "Justify texture",
        "Justify the texture of faces: fit, left, right, top, bottom or center, each face or treated as one.",
        {
            "faces": faces("Faces (default: the selected faces, else the faces of ids)."),
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "mode": choice("Justification.", &["fit", "left", "right", "top", "bottom", "center"]),
            "treat_as_one": boolean("Treat the faces as one (default false)."),
        }, ["mode"];
    fit_texture: Edit, "Fit texture",
        "Fit the texture of faces so it repeats a number of times on U and V.",
        {
            "faces": faces("Faces (default: the selected faces, else the faces of ids)."),
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "repeat_u": number("Repeats on U (default 1).", 0.01, 1024.0),
            "repeat_v": number("Repeats on V (default 1).", 0.01, 1024.0),
            "treat_as_one": boolean("Treat the faces as one (default false)."),
        }, [];
    align_texture_to_view: Edit, "Align texture to a view",
        "Project the texture of faces from a 2D view direction (top, front or side) or explicit right and up axes.",
        {
            "faces": faces("Faces (default: the selected faces, else the faces of ids)."),
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "view": choice("2D view.", &["top", "front", "side"]),
            "right": vec3("Texture right axis (with up)."),
            "up": vec3("Texture up axis (with right)."),
        }, [];
    wrap_texture: Edit, "Wrap texture",
        "Wrap the texture of a source face onto other faces so it continues across edges (Hammer++ alt+right click).",
        {
            "source": faces("The source face (one item)."),
            "faces": faces("Faces that get the texture."),
        }, ["source", "faces"];
    replace_materials: Edit, "Replace materials",
        "Replace a material (pattern with * and ?) with another on every face of the map, of the selection, or of \
        some objects.",
        {
            "find": string("Material pattern."),
            "replace": string("New material."),
            "scope": choice("Where (default all).", &["all", "selection", "ids"]),
            "ids": target_ids(), "solid_ids": target_solid_ids(),
        }, ["find", "replace"];
    list_used_materials: Read, "Used materials",
        "Materials used by the faces of the map with their face counts, most used first.",
        { "limit": limit(), "offset": offset() }, [];
    random_rotate_objects: Edit, "Random Z rotation",
        "Turn each point entity with an angles key (prop_static, prop_dynamic, prop_physics, lights...) by its own \
        random yaw around Z, in one undo step, like Tools > Random Z rotation. Brushes, brush entities and \
        entities without angles are skipped and counted. The options default to the selection.random_* settings.",
        {
            "ids": target_ids(), "solid_ids": target_solid_ids(),
            "min_yaw": number("Smallest turn in degrees (default selection.random_yaw_min, 0).", -360.0, 360.0),
            "max_yaw": number("Largest turn in degrees (default selection.random_yaw_max, 360).", -360.0, 720.0),
            "snap": number("Turn in steps of this many degrees, 0 free (default selection.random_yaw_snap).", 0.0, 180.0),
            "tilt": number("Random pitch and roll jitter up to this many degrees, 0 off (default: off unless selection.random_tilt).", 0.0, hp_editor::MAX_JITTER_DEG),
            "seed": integer("Random seed (default: a new one each call).", 0, i64::from(u32::MAX)),
        }, [];
}

fn random_rotate_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let seed = a
        .opt_u64("seed", 0, u64::from(u32::MAX))?
        .unwrap_or_else(|| hp_app::settings::SelectionSettings::random_seed() % (u64::from(u32::MAX) + 1));
    let mut opts = state.settings.selection.random_rotate(seed);
    if let Some(v) = a.opt_f64("min_yaw", -360.0, 360.0)? {
        opts.min_yaw = v;
    }
    if let Some(v) = a.opt_f64("max_yaw", -360.0, 720.0)? {
        opts.max_yaw = v;
    }
    if let Some(v) = a.opt_f64("snap", 0.0, 180.0)? {
        opts.snap = v;
    }
    if let Some(v) = a.opt_f64("tilt", 0.0, hp_editor::MAX_JITTER_DEG)? {
        opts.jitter = v;
    }
    opts.check().map_err(|e| e.to_string())?;
    let r = edit(state, "random_rotate_objects", |doc| doc.random_rotate(&objs, &opts))?;
    let scene = state.doc.scene();
    let angles: Vec<Value> = objs
        .iter()
        .filter_map(|o| match *o {
            ObjRef::Entity(id) => {
                scene.entity(id).and_then(|n| n.entity.angles()).map(|a| json!({ "id": id, "angles": v3(a) }))
            }
            ObjRef::Solid(_) => None,
        })
        .take(200)
        .collect();
    Ok(ToolResult::json(json!({
        "rotated": r.rotated,
        "skipped": r.skipped,
        "seed": seed,
        "options": { "min_yaw": opts.min_yaw, "max_yaw": opts.max_yaw, "snap": opts.snap, "tilt": opts.jitter },
        "angles": angles,
    })))
}

fn objs_json(objs: &[ObjRef]) -> Value {
    Value::Array(objs.iter().map(|o| obj_json(*o)).collect())
}

fn objs_bounds(state: &AppState, objs: &[ObjRef]) -> R<hp_geom::Aabb> {
    let f = bounds_fn(state);
    state
        .doc
        .scene()
        .objects_bounds(objs, &f)
        .filter(|b| !b.is_empty())
        .ok_or_else(|| "the objects have no bounds".to_owned())
}

fn rotation(deg: DVec3) -> DQuat {
    DQuat::from_rotation_z(deg.z.to_radians())
        * DQuat::from_rotation_y(deg.y.to_radians())
        * DQuat::from_rotation_x(deg.x.to_radians())
}

fn opt_material(state: &AppState, a: &Args<'_>, key: &str) -> R<String> {
    match a.opt_str(key, MAX_NAME)? {
        Some(m) => material_name(m),
        None => Ok(state.current_material.clone()),
    }
}

fn texture_sizes(state: &mut AppState, faces: &[FaceRef]) -> HashMap<String, (u32, u32)> {
    let mut names: Vec<String> =
        faces.iter().filter_map(|f| state.doc.face_side(*f)).map(|s| s.material.to_ascii_lowercase()).collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .filter_map(|n| {
            let info = state.assets.material_info(&n)?;
            (info.width > 0 && info.height > 0).then(|| (n, (info.width, info.height)))
        })
        .collect()
}

fn duplicate_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let g = grid(state);
    let offset = a.opt_vec3("offset")?.unwrap_or(DVec3::new(g, g, 0.0));
    let AppState { doc, assets, .. } = state;
    if doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    let fgd = assets.fgd().filter(|f| !f.is_empty());
    let targets = |class: &str, key: &str| fgd.and_then(|f| hp_app::runtime::helpers::fgd_target_key(f, class, key));
    let copies = doc
        .transaction(super::tools::undo_label("duplicate_objects"), |doc| {
            doc.duplicate_with(&objs, offset, Some(&targets))
        })
        .map_err(|e| e.to_string())?;
    Ok(ToolResult::json(json!({ "copies": objs_json(&copies) })))
}

fn grid_array(state: &mut AppState, a: &Args<'_>, objs: &[ObjRef]) -> R<ToolResult> {
    let names = a.strings("axes", 3)?;
    let mut enabled = [false; 3];
    for n in &names {
        match n.to_ascii_lowercase().as_str() {
            "x" => enabled[0] = true,
            "y" => enabled[1] = true,
            "z" => enabled[2] = true,
            other => return Err(format!("axes: \"{other}\" is not x, y or z")),
        }
    }
    let per_axis = a.opt_u64("count", 1, u64::from(hp_editor::MAX_ARRAY_AXIS_COUNT))?;
    let counts = match a.get("counts") {
        Some(v) => {
            let arr = v.as_array().filter(|x| x.len() == 3).ok_or("counts must be [nx, ny, nz]")?;
            let mut out = [0u32; 3];
            for (o, x) in out.iter_mut().zip(arr) {
                let n = x.as_u64().filter(|n| *n <= u64::from(hp_editor::MAX_ARRAY_AXIS_COUNT));
                *o = n.ok_or("counts must be integers from 0 to 1024")? as u32;
            }
            Some(out)
        }
        None => None,
    };
    let mut axes = [hp_editor::ArrayAxis::default(); 3];
    for k in 0..3 {
        let on = names.is_empty() || enabled[k];
        axes[k].count = match counts {
            Some(c) if on => c[k],
            Some(_) => 0,
            None if enabled[k] => per_axis.ok_or("count is required with axes")? as u32,
            None => 0,
        };
    }
    let gap = a.opt_f64("gap", -131072.0, 131072.0)?.unwrap_or(0.0);
    let gaps = a.opt_vec3("gaps")?.unwrap_or(DVec3::splat(gap));
    for k in 0..3 {
        axes[k].gap = gaps[k];
    }
    let mut frame = hp_app::runtime::helpers::array_frame(state);
    if !a.opt_bool("snap")?.unwrap_or(state.snap_enabled()) {
        frame.step = DVec3::ZERO;
    }
    let spec = hp_editor::ArraySpec { axes, frame };
    let AppState { doc, assets, .. } = state;
    if doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    let fgd = assets.fgd().filter(|f| !f.is_empty());
    let targets = |class: &str, key: &str| fgd.and_then(|f| hp_app::runtime::helpers::fgd_target_key(f, class, key));
    let (plan, copies) = doc
        .transaction(super::tools::undo_label("array_duplicate"), |doc| {
            doc.array_copies(objs, &spec, &|e| assets.entity_bounds(e), Some(&targets))
        })
        .map_err(|e| e.to_string())?;
    Ok(ToolResult::json(json!({
        "copies": objs_json(&copies),
        "steps": v3(plan.steps),
        "extent": v3(plan.extent),
        "snapped_to_grid": spec.frame.step != DVec3::ZERO,
    })))
}

fn array_duplicate(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    if a.get("axes").is_some() || a.get("counts").is_some() {
        return grid_array(state, a, &objs);
    }
    let count = a.opt_u64("count", 1, 256)?.ok_or("count is required")? as u32;
    let offset = a.opt_vec3("offset")?.unwrap_or(DVec3::ZERO);
    let rot = a.opt_vec3("rotate")?.unwrap_or(DVec3::ZERO);
    let scale = a.opt_f64("scale", 0.01, 100.0)?.unwrap_or(1.0);
    let pivot = match a.opt_vec3("pivot")? {
        Some(p) => p,
        None => objs_bounds(state, &objs)?.center(),
    };
    if offset == DVec3::ZERO && rot == DVec3::ZERO && (scale - 1.0).abs() < 1e-9 {
        return Err("give an offset, a rotation or a scale (the copies would all be in the same place)".to_owned());
    }
    let step = rotation(rot);
    let copies = edit(state, "array_duplicate", |doc| {
        let mut all = Vec::new();
        for i in 1..=count {
            let n = f64::from(i);
            let m = DMat4::from_translation(pivot + offset * n)
                * DMat4::from_quat(DQuat::from_scaled_axis(step.to_scaled_axis() * n))
                * DMat4::from_scale(DVec3::splat(scale.powf(n)))
                * DMat4::from_translation(-pivot);
            all.extend(doc.duplicate_transformed(&objs, m, pivot, true)?);
        }
        doc.select(&all, SelectOp::Replace);
        Ok(all)
    })?;
    Ok(ToolResult::json(json!({ "copies": objs_json(&copies) })))
}

fn transform_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let t = a.opt_vec3("translate")?.unwrap_or(DVec3::ZERO);
    let r = a.opt_vec3("rotate")?.unwrap_or(DVec3::ZERO);
    let s = a.opt_vec3("scale")?.unwrap_or(DVec3::ONE);
    if s.to_array().iter().any(|x| !(0.001..=1000.0).contains(x)) {
        return Err("scale factors must be from 0.001 to 1000 (flip_objects mirrors)".to_owned());
    }
    if r.to_array().iter().any(|x| x.abs() > 3600.0) {
        return Err("rotate must be degrees from -3600 to 3600".to_owned());
    }
    let pivot = match a.opt_vec3("pivot")? {
        Some(p) => p,
        None => objs_bounds(state, &objs)?.center(),
    };
    let m = DMat4::from_translation(pivot + t)
        * DMat4::from_quat(rotation(r))
        * DMat4::from_scale(s)
        * DMat4::from_translation(-pivot);
    edit(state, "transform_objects", |doc| doc.transform(&objs, m))?;
    let b = objs_bounds(state, &objs).ok();
    Ok(ToolResult::json(json!({ "objects": objs.len(), "bounds": bounds_json(b) })))
}

fn resize_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let to = hp_geom::Aabb::new(a.vec3("min")?, a.vec3("max")?);
    if to.size().min_element() < 0.01 {
        return Err("the box must be at least 0.01 unit on every axis".to_owned());
    }
    let from = objs_bounds(state, &objs)?;
    if from.size().min_element() < 1e-6 {
        return Err("the objects are flat on an axis and cannot be resized".to_owned());
    }
    edit(state, "resize_objects", |doc| doc.scale_bounds(&objs, &from, &to))?;
    Ok(ToolResult::json(json!({ "objects": objs.len(), "bounds": bounds_json(objs_bounds(state, &objs).ok()) })))
}

fn flip_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let axis = opt_axis(a, "axis")?.ok_or("axis is required")?;
    let center = match a.opt_vec3("center")? {
        Some(c) => c,
        None => objs_bounds(state, &objs)?.center(),
    };
    edit(state, "flip_objects", |doc| doc.flip(&objs, axis, center))?;
    Ok(ToolResult::json(json!({ "objects": objs.len(), "center": v3(center) })))
}

fn snap_objects_to_grid(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let g = a.opt_f64("grid", 0.125, 4096.0)?.unwrap_or_else(|| grid(state));
    let each = a.opt_bool("individually")?.unwrap_or(false);
    let AppState { doc, assets, .. } = state;
    let f = |e: &hp_vmf::Entity| assets.entity_bounds(e);
    if doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    doc.transaction(super::tools::undo_label("snap_objects_to_grid"), |doc| doc.snap_to_grid(&objs, g, each, &f))
        .map_err(|e| e.to_string())?;
    Ok(ToolResult::json(json!({ "objects": objs.len(), "grid": g })))
}

fn align_objects(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let axis = opt_axis(a, "axis")?.ok_or("axis is required")?;
    let mode = match a.opt_enum("mode", &["min", "max", "center"])?.as_deref() {
        Some("min") => AlignMode::Min,
        Some("max") => AlignMode::Max,
        _ => AlignMode::Center,
    };
    let individually = a.opt_bool("individually")?.unwrap_or(true);
    let reference = match (a.opt_u64("reference_id", 1, u64::from(u32::MAX))?, a.opt_f64("value", -131072.0, 131072.0)?)
    {
        (Some(_), Some(_)) => return Err("give reference_id or value, not both".to_owned()),
        (Some(id), None) => {
            let id = id as u32;
            let obj = if state.doc.scene().entity(id).is_some() {
                ObjRef::Entity(id)
            } else if state.doc.scene().solid(id).is_some() {
                ObjRef::Solid(id)
            } else {
                return Err(format!("no entity or solid has the id {id}"));
            };
            objs_bounds(state, &[obj])?
        }
        (None, Some(v)) => {
            let mut p = DVec3::ZERO;
            p[axis] = v;
            hp_geom::Aabb::new(p, p)
        }
        (None, None) => objs_bounds(state, &objs)?,
    };
    let AppState { doc, assets, .. } = state;
    let f = |e: &hp_vmf::Entity| assets.entity_bounds(e);
    if doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    doc.transaction(super::tools::undo_label("align_objects"), |doc| {
        doc.align_objects(&objs, &reference, axis, mode, individually, &f)
    })
    .map_err(|e| e.to_string())?;
    Ok(ToolResult::json(json!({ "objects": objs.len() })))
}

fn drop_to_surface(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let dir = opt_dir(a, "direction")?.unwrap_or(DVec3::NEG_Z);
    let align = a.opt_bool("align")?.unwrap_or(false);
    let AppState { doc, assets, .. } = state;
    let f = |e: &hp_vmf::Entity| assets.entity_bounds(e);
    if doc.in_transaction() {
        return Err("Hammer Pro is in the middle of another edit; try again".to_owned());
    }
    let moved = doc
        .transaction(super::tools::undo_label("drop_to_surface"), |doc| doc.drop_to_surface(&objs, dir, align, &f))
        .map_err(|e| e.to_string())?;
    Ok(ToolResult::json(json!({ "moved": moved, "objects": objs.len() })))
}

fn clip_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let solids = target_solids(state, a, "solid_ids")?;
    let plane = match (a.get("points"), a.opt_vec3("normal")?) {
        (Some(p), None) => {
            let arr = p.as_array().filter(|x| x.len() == 3).ok_or("points must be three [x, y, z] points")?;
            let mut pts = [DVec3::ZERO; 3];
            for (o, v) in pts.iter_mut().zip(arr) {
                let xyz = v.as_array().filter(|x| x.len() == 3).ok_or("points must be three [x, y, z] points")?;
                let c: Vec<f64> =
                    xyz.iter().filter_map(Value::as_f64).filter(|f| f.is_finite() && f.abs() <= 131072.0).collect();
                if c.len() != 3 {
                    return Err("points must be three [x, y, z] points with numbers from -131072 to 131072".to_owned());
                }
                *o = DVec3::new(c[0], c[1], c[2]);
            }
            hp_geom::Plane::from_points(pts[0], pts[1], pts[2]).ok_or("the three points are on one line")?
        }
        (None, Some(n)) => {
            let n = n.try_normalize().ok_or("normal must not be zero")?;
            match (a.opt_vec3("point")?, a.opt_f64("distance", -262144.0, 262144.0)?) {
                (Some(_), Some(_)) => return Err("give point or distance with normal, not both".to_owned()),
                (Some(p), None) => hp_geom::Plane::from_normal_point(n, p).ok_or("invalid plane")?,
                (None, Some(d)) => hp_geom::Plane::new(n, d).ok_or("invalid plane")?,
                (None, None) => return Err("give point or distance with normal".to_owned()),
            }
        }
        _ => return Err("give points, a point and a normal, or a normal and a distance".to_owned()),
    };
    let keep = match a.opt_enum("keep", &["front", "back", "both"])?.as_deref() {
        Some("front") => ClipKeep::Front,
        Some("back") => ClipKeep::Back,
        _ => ClipKeep::Both,
    };
    let cap = a.opt_str("cap_material", MAX_NAME)?.map(material_name).transpose()?;
    let ids = edit(state, "clip_brushes", |doc| doc.clip_with(&solids, &plane, keep, cap.as_deref()))?;
    Ok(ToolResult::json(json!({ "solid_ids": ids })))
}

fn carve(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let carvers = target_solids(state, a, "solid_ids")?;
    let n = edit(state, "carve", |doc| doc.carve(&carvers))?;
    Ok(ToolResult::json(json!({ "carved": n })))
}

fn extrude_face(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let solid = solid_id(&state.doc, a, "solid")?;
    let side = a.opt_u64("side", 0, 4095)?.ok_or("side is required")? as usize;
    let sides = state.doc.scene().solid(solid).map_or(0, |n| n.solid.sides.len());
    if side >= sides {
        return Err(format!("solid {solid} has {sides} sides (0 to {})", sides.saturating_sub(1)));
    }
    let list =
        a.get("points").and_then(Value::as_array).filter(|l| (3..=hp_geom::MAX_OUTLINE_POINTS).contains(&l.len()));
    let list = list.ok_or_else(|| format!("points must be 3 to {} [x, y, z] points", hp_geom::MAX_OUTLINE_POINTS))?;
    let mut points = Vec::with_capacity(list.len());
    for v in list {
        let c: Vec<f64> = v
            .as_array()
            .filter(|x| x.len() == 3)
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .filter(|f| f.is_finite() && f.abs() <= 131072.0)
            .collect();
        if c.len() != 3 {
            return Err("points must contain [x, y, z] points with numbers from -131072 to 131072".to_owned());
        }
        points.push(DVec3::new(c[0], c[1], c[2]));
    }
    let mode = match a.opt_enum("mode", &["carve", "extrude"])?.as_deref() {
        Some("extrude") => hp_geom::ExtrudeMode::Extrude,
        _ => hp_geom::ExtrudeMode::Carve,
    };
    let depth = a.opt_f64("depth", hp_geom::MIN_EXTRUDE_DEPTH, 16384.0)?.unwrap_or(16.0);
    let through = a.opt_bool("through")?.unwrap_or(false);
    let material = match (a.opt_str("material", MAX_NAME)?, a.opt_enum("new_faces", &["face", "current"])?.as_deref()) {
        (Some(m), _) => Some(material_name(m)?),
        (None, Some("current")) => Some(opt_material(state, a, "material")?).filter(|m| !m.trim().is_empty()),
        _ => None,
    };
    let params = hp_geom::FaceExtrudeParams { mode, depth, through, material };
    let face = FaceRef::new(solid, side);
    let ids = edit(state, "extrude_face", |doc| doc.extrude_face(face, &points, &params))?;
    let bounds =
        state.doc.scene().objects_bounds(&ids.iter().map(|id| ObjRef::Solid(*id)).collect::<Vec<_>>(), &|_| None);
    Ok(ToolResult::json(json!({
        "solid_ids": ids,
        "mode": mode.id(),
        "brushes": ids.len(),
        "bounds": bounds_json(bounds),
    })))
}

fn face_reference(state: &AppState, face: &FaceRef) -> Option<(DVec3, DVec3)> {
    let g = state.doc.scene().solid(face.solid)?.geom.face_for_side(face.side)?;
    Some((g.vertices.first().copied()?, g.plane.normal))
}

fn inset_face(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = target_faces(state, a, "faces")?;
    let mut distance = a.opt_f64("distance", hp_geom::MIN_INSET, hp_geom::MAX_INSET)?.unwrap_or(16.0);
    let mut depth = a.opt_f64("depth", -hp_geom::MAX_INSET, hp_geom::MAX_INSET)?.unwrap_or(0.0);
    let individual = a.opt_bool("individual")?.unwrap_or(false);
    if a.opt_bool("snap")?.unwrap_or(false) {
        let frame = state.target_grid().frame;
        match faces.first().and_then(|f| face_reference(state, f)) {
            Some((from, normal)) => {
                let step = frame.plane_step(normal);
                distance = ((distance / step).round() * step).max(step.max(hp_geom::MIN_INSET));
                depth = frame.snap_distance(from, normal, depth);
            }
            None => {
                let g = hp_app::settings::tool_grid(state.grid_size());
                distance = ((distance / g).round() * g).max(g);
                depth = (depth / g).round() * g;
            }
        }
    }
    let material = a.opt_str("material", MAX_NAME)?.map(material_name).transpose()?;
    let params = hp_geom::InsetParams { distance, depth, material, individual };
    if a.opt_bool("dry_run")?.unwrap_or(false) {
        let plan = state.doc.inset_preview(&faces, &params).map_err(|e| e.to_string())?;
        let brushes: usize = plan.iter().map(|(_, p)| p.len()).sum();
        let answer = json!({
            "dry_run": true,
            "brushes": brushes,
            "distance": distance,
            "depth": depth,
            "individual": individual,
        });
        return Ok(ToolResult::json(answer));
    }
    let ids = edit(state, "inset_face", |doc| doc.inset_faces(&faces, &params))?;
    let bounds =
        state.doc.scene().objects_bounds(&ids.iter().map(|id| ObjRef::Solid(*id)).collect::<Vec<_>>(), &|_| None);
    Ok(ToolResult::json(json!({
        "solid_ids": ids,
        "brushes": ids.len(),
        "distance": distance,
        "depth": depth,
        "individual": individual,
        "bounds": bounds_json(bounds),
    })))
}

fn extrude_faces(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = target_faces(state, a, "faces")?;
    let distance = a.opt_f64("distance", -16384.0, 16384.0)?.unwrap_or(16.0);
    let snap = a.opt_bool("snap")?.unwrap_or(false);
    let alt = a.opt_bool("alt_grid")?.unwrap_or(false);
    let material = a.opt_str("material", MAX_NAME)?.map(material_name).transpose()?;
    let base = state.target_grid().frame;
    let frame = if alt { base.alternate() } else { base };
    let mut list: Vec<(FaceRef, f64)> = Vec::with_capacity(faces.len());
    for f in &faces {
        let d = match face_reference(state, f) {
            Some((from, normal)) if snap || alt => frame.snap_distance(from, normal, distance),
            _ => distance,
        };
        list.push((*f, d));
    }
    list.retain(|(_, d)| d.abs() >= hp_geom::MIN_EXTRUDE_DEPTH);
    if list.is_empty() {
        return Err("the distance snaps to less than 0.125 on every face".to_owned());
    }
    let depths: Vec<Value> =
        list.iter().map(|(f, d)| json!({ "solid": f.solid, "side": f.side, "distance": d })).collect();
    if a.opt_bool("dry_run")?.unwrap_or(false) {
        let n = state.doc.extrude_faces_preview(&list, material.as_deref()).map_err(|e| e.to_string())?;
        return Ok(ToolResult::json(json!({ "dry_run": true, "brushes": n, "faces": depths })));
    }
    let ids = edit(state, "extrude_faces", |doc| doc.extrude_faces(&list, material.as_deref()))?;
    let bounds =
        state.doc.scene().objects_bounds(&ids.iter().map(|id| ObjRef::Solid(*id)).collect::<Vec<_>>(), &|_| None);
    Ok(ToolResult::json(json!({
        "solid_ids": ids,
        "brushes": ids.len(),
        "faces": depths,
        "bounds": bounds_json(bounds),
    })))
}

fn get_brush_edges(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = solid_id(&state.doc, a, "solid")?;
    let n = state.doc.scene().solid(id).filter(|n| n.geom.valid).ok_or("the solid has no valid geometry")?;
    let edges: Vec<Value> = n
        .geom
        .edges()
        .iter()
        .enumerate()
        .map(|(i, [p, q])| json!({ "index": i, "a": v3(*p), "b": v3(*q), "length": p.distance(*q) }))
        .collect();
    Ok(ToolResult::json(json!({ "solid": id, "edges": edges })))
}

const MAX_BEVEL_EDGES: usize = 64;
const MAX_EXPANDED_EDGES: usize = 512;

fn edge_pair(v: &Value) -> Option<[DVec3; 2]> {
    let list = v.as_array().filter(|l| l.len() == 2)?;
    let point = |p: &Value| -> Option<DVec3> {
        let c: Vec<f64> = p.as_array().filter(|c| c.len() == 3)?.iter().filter_map(Value::as_f64).collect();
        (c.len() == 3 && c.iter().all(|x| super::tools::coord_ok(*x))).then(|| DVec3::new(c[0], c[1], c[2]))
    };
    Some([point(&list[0])?, point(&list[1])?])
}

fn indexed_edge(state: &AppState, solid: u32, v: &Value) -> R<hp_editor::EdgeRef> {
    let n = state.doc.scene().solid(solid).ok_or_else(|| format!("no solid has the id {solid}"))?;
    let edges = if n.geom.valid { n.geom.edges() } else { Vec::new() };
    let i = v.as_u64().map(|i| i as usize).filter(|i| *i < edges.len());
    let i = i.ok_or_else(|| format!("solid {solid} has {} edges (0 to {})", edges.len(), edges.len().max(1) - 1))?;
    Ok(hp_editor::EdgeRef::new(solid, edges[i][0], edges[i][1]))
}

fn bevel_edge_item(state: &AppState, solid: Option<u32>, v: &Value) -> R<hp_editor::EdgeRef> {
    let bad = "each edge is an index, [[x, y, z], [x, y, z]], {solid, edge} or {solid, a, b}";
    let need = || solid.ok_or_else(|| "give solid for edges given as indices or points".to_owned());
    match v {
        Value::Number(_) => indexed_edge(state, need()?, v),
        Value::Array(_) => {
            let [p, q] = edge_pair(v).ok_or(bad)?;
            Ok(hp_editor::EdgeRef::new(need()?, p, q))
        }
        Value::Object(o) => {
            let solid = match o.get("solid") {
                Some(s) => {
                    let id = s.as_u64().and_then(|x| u32::try_from(x).ok()).ok_or("solid must be a solid id")?;
                    if state.doc.scene().solid(id).is_none() {
                        return Err(format!("no solid has the id {id}"));
                    }
                    id
                }
                None => need()?,
            };
            match (o.get("edge"), o.get("a"), o.get("b")) {
                (Some(e), None, None) => indexed_edge(state, solid, e),
                (None, Some(p), Some(q)) => {
                    let [p, q] = edge_pair(&json!([p, q])).ok_or(bad)?;
                    Ok(hp_editor::EdgeRef::new(solid, p, q))
                }
                _ => Err(bad.to_owned()),
            }
        }
        _ => Err(bad.to_owned()),
    }
}

fn bevel_edges(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let solid = if a.get("solid").is_some() { Some(solid_id(&state.doc, a, "solid")?) } else { None };
    let mut items: Vec<&Value> = a.get("edge").into_iter().collect();
    if let Some(list) = a.get("edges") {
        items.extend(list.as_array().ok_or("edges must be an array")?);
    }
    if items.is_empty() {
        return Err("give edge or edges".to_owned());
    }
    if items.len() > MAX_BEVEL_EDGES {
        return Err(format!("at most {MAX_BEVEL_EDGES} edges per call"));
    }
    let mut edges: Vec<hp_editor::EdgeRef> = Vec::with_capacity(items.len());
    for v in items {
        let e = bevel_edge_item(state, solid, v)?;
        if !edges.iter().any(|x| x.same_edge(&e)) {
            edges.push(e);
        }
    }
    let radius = a.opt_f64("radius", hp_geom::MIN_BEVEL_RADIUS, hp_geom::MAX_BEVEL_RADIUS)?.unwrap_or(16.0);
    let segments = a.opt_u64("segments", 1, u64::from(hp_geom::MAX_BEVEL_SEGMENTS))?.unwrap_or(4) as u32;
    let inverse = a.opt_bool("inverse")?.unwrap_or(false);
    let edges = match a.opt_enum("expand", &["none", "loop", "ring"])?.as_deref() {
        Some(kind @ ("loop" | "ring")) => {
            let kind = if kind == "ring" { hp_editor::RunKind::Ring } else { hp_editor::RunKind::Loop };
            let selected = state.doc.selected_solid_ids();
            let mut out: Vec<hp_editor::EdgeRef> = Vec::new();
            for seed in &edges {
                let run = state.doc.edge_run_refs(kind, seed, &selected, None).map_err(|e| e.to_string())?;
                let mut any = false;
                for e in run {
                    if state.doc.bevel_opening(&e, inverse).is_some() && !out.iter().any(|x| x.same_edge(&e)) {
                        out.push(e);
                        any = true;
                    }
                }
                if !any && !out.iter().any(|x| x.same_edge(seed)) {
                    out.push(*seed);
                }
            }
            if out.len() > MAX_EXPANDED_EDGES {
                return Err(format!(
                    "the loops have {} edges, at most {MAX_EXPANDED_EDGES} can be bevelled at once",
                    out.len()
                ));
            }
            out
        }
        _ => edges,
    };
    let material = match (a.opt_str("material", MAX_NAME)?, a.opt_enum("new_faces", &["face", "current"])?.as_deref()) {
        (Some(m), _) => Some(material_name(m)?),
        (None, Some("current")) => Some(opt_material(state, a, "material")?).filter(|m| !m.trim().is_empty()),
        _ => None,
    };
    let params = hp_geom::BevelParams { radius, segments, inverse, material };
    let largest = state.doc.bevel_limit(&edges, inverse).map_err(|e| e.to_string())?;
    if a.opt_bool("dry_run")?.unwrap_or(false) {
        let mut next = 1u32;
        let list = state
            .doc
            .bevel_preview(&edges, &params, &mut || {
                next = next.wrapping_add(1);
                next
            })
            .map_err(|e| e.to_string())?;
        let kinds: Vec<&str> = list.iter().flat_map(|(_, r)| r.kinds.iter().map(|k| k.id())).collect();
        let brushes: usize =
            list.iter().map(|(_, r)| r.added.len() + if r.changes_brush() { r.pieces.len() } else { 1 }).sum();
        return Ok(ToolResult::json(json!({
            "dry_run": true,
            "radius": list.iter().map(|(_, r)| r.radius).fold(f64::INFINITY, f64::min),
            "segments": list.iter().map(|(_, r)| r.segments).min().unwrap_or(1),
            "max_radius": largest,
            "kinds": kinds,
            "brushes": brushes,
        })));
    }
    let out = edit(state, "bevel_edges", |doc| doc.bevel_edges(&edges, &params))?;
    let bounds =
        state.doc.scene().objects_bounds(&out.ids.iter().map(|id| ObjRef::Solid(*id)).collect::<Vec<_>>(), &|_| None);
    Ok(ToolResult::json(json!({
        "solid_ids": out.ids,
        "radius": out.radius,
        "segments": out.segments,
        "max_radius": largest,
        "kinds": out.kinds.iter().map(|k| k.id()).collect::<Vec<_>>(),
        "brushes": out.ids.len(),
        "bounds": bounds_json(bounds),
    })))
}

fn hollow(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let solids = target_solids(state, a, "solid_ids")?;
    let t = a.opt_f64("thickness", -1024.0, 1024.0)?.unwrap_or(16.0);
    if t.abs() < 0.5 {
        return Err("thickness must be at least 0.5 unit (or -0.5)".to_owned());
    }
    let walls = edit(state, "hollow", |doc| doc.hollow(&solids, t))?;
    Ok(ToolResult::json(json!({ "wall_ids": walls })))
}

fn merge_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let solids = target_solids(state, a, "solid_ids")?;
    if solids.len() < 2 {
        return Err("merging needs at least two solids".to_owned());
    }
    let hull = a.opt_bool("convex_hull")?.unwrap_or(false);
    let id =
        edit(state, "merge_brushes", |doc| if hull { doc.merge_solids(&solids) } else { doc.merge_brushes(&solids) })?;
    Ok(ToolResult::json(json!({ "solid_id": id, "merged": solids.len() })))
}

fn intersect_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let solids = target_solids(state, a, "solid_ids")?;
    if solids.len() < 2 {
        return Err("intersecting needs at least two solids".to_owned());
    }
    let id = edit(state, "intersect_brushes", |doc| doc.intersect_solids(&solids))?;
    Ok(ToolResult::json(json!({ "solid_id": id, "intersected": solids.len() })))
}

fn union_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let solids = target_solids(state, a, "solid_ids")?;
    if solids.len() < 2 {
        return Err("a union needs at least two solids".to_owned());
    }
    let ids = edit(state, "union_brushes", |doc| doc.union_solids(&solids))?;
    Ok(ToolResult::json(json!({ "solid_ids": ids })))
}

fn get_brush_vertices(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = solid_id(&state.doc, a, "solid")?;
    let (points, faces) = state.doc.solid_vertices(id).ok_or("the solid has no valid geometry")?;
    Ok(ToolResult::json(json!({
        "solid": id,
        "vertices": points.iter().map(|p| v3(*p)).collect::<Vec<_>>(),
        "faces": faces,
    })))
}

fn move_brush_vertices(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = solid_id(&state.doc, a, "solid")?;
    let offset = a.vec3("offset")?;
    let uv_lock = a.opt_bool("uv_lock")?.unwrap_or(true);
    let (base, _) = state.doc.solid_vertices(id).ok_or("the solid has no valid geometry")?;
    let list = a.get("vertices").and_then(Value::as_array).ok_or("vertices must be an array of indices")?;
    let mut points = base.clone();
    for v in list {
        let i = v.as_u64().map(|i| i as usize).filter(|i| *i < base.len());
        let i = i.ok_or_else(|| format!("vertices must be indices from 0 to {}", base.len().saturating_sub(1)))?;
        points[i] = base[i] + offset;
    }
    let vertex_edit = hp_geom::VertexEdit { points: &points, base: Some(&base), topology: None };
    let opts = hp_geom::VertexEditOptions { uv_lock };
    edit(state, "move_brush_vertices", |doc| doc.edit_solid_vertices(id, &vertex_edit, opts))?;
    let (after, _) = state.doc.solid_vertices(id).unwrap_or_default();
    Ok(ToolResult::json(json!({ "solid": id, "vertices": after.iter().map(|p| v3(*p)).collect::<Vec<_>>() })))
}

fn points_arg(a: &Args<'_>, key: &str) -> R<Vec<DVec3>> {
    let list = a.get(key).and_then(Value::as_array).filter(|l| (4..=4096).contains(&l.len()));
    let list = list.ok_or_else(|| format!("{key} must be 4 to 4096 [x, y, z] points"))?;
    list.iter()
        .map(|v| {
            let xyz = v.as_array().filter(|x| x.len() == 3);
            let c: Vec<f64> = xyz
                .into_iter()
                .flatten()
                .filter_map(Value::as_f64)
                .filter(|f| f.is_finite() && f.abs() <= 131072.0)
                .collect();
            if c.len() == 3 {
                Ok(DVec3::new(c[0], c[1], c[2]))
            } else {
                Err(format!("{key} must contain [x, y, z] points with numbers from -131072 to 131072"))
            }
        })
        .collect()
}

fn set_brush_vertices(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let id = solid_id(&state.doc, a, "solid")?;
    let points = points_arg(a, "points")?;
    edit(state, "set_brush_vertices", |doc| doc.set_solid_vertices(id, &points))?;
    let (after, faces) = state.doc.solid_vertices(id).unwrap_or_default();
    Ok(ToolResult::json(json!({ "solid": id, "vertices": after.len(), "faces": faces.len() })))
}

fn create_brush_from_points(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let points = points_arg(a, "points")?;
    let material = opt_material(state, a, "material")?;
    let class = a.opt_str("entity_class", MAX_NAME)?.map(str::trim).map(str::to_owned);
    if let Some(c) = &class {
        check_class(c)?;
    }
    let bounds = hp_geom::Aabb::from_points(points.iter().copied());
    if bounds.size().min_element() < 0.5 {
        return Err("the points are flat (the solid needs some volume on every axis)".to_owned());
    }
    let params = hp_geom::PrimitiveParams {
        kind: hp_geom::Primitive::Block,
        bounds,
        material: material.clone(),
        ..Default::default()
    };
    let origin =
        class.as_deref().is_some_and(|c| hp_app::runtime::helpers::brush_entity_needs_origin(state.assets.fgd(), c));
    let (solid, entity) = edit(state, "create_brush_from_points", |doc| {
        let ids = doc.create_primitive(&params, Owner::World)?;
        let id = *ids.first().ok_or(hp_editor::EditError::Nothing("no solid created"))?;
        doc.set_solid_vertices(id, &points)?;
        let entity = match &class {
            Some(c) => {
                let e = doc.tie_to_entity(&[ObjRef::Solid(id)], c, &[])?;
                if origin {
                    doc.add_missing_origins(&[e])?;
                }
                Some(e)
            }
            None => None,
        };
        let sel = entity.map_or(ObjRef::Solid(id), ObjRef::Entity);
        doc.select(&[sel], SelectOp::Replace);
        Ok((id, entity))
    })?;
    Ok(ToolResult::json(json!({ "solid_id": solid, "entity_id": entity, "material": material })))
}

struct Made {
    ids: Vec<u32>,
    entity: Option<u32>,
    bounds: hp_geom::Aabb,
    material: String,
}

fn create_primitive(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let made = make_shape(state, a, "create_primitive", false)?;
    Ok(ToolResult::json(json!({
        "solid_ids": made.ids,
        "entity_id": made.entity,
        "material": made.material,
        "bounds": super::tools::bounds_json(Some(made.bounds)),
    })))
}

fn make_shape(state: &mut AppState, a: &Args<'_>, label: &str, group: bool) -> R<Made> {
    let kind = match a
        .opt_enum(
            "shape",
            &[
                "block",
                "wedge",
                "cylinder",
                "spike",
                "sphere",
                "arch",
                "torus",
                "stairs",
                "spiral_stairs",
                "ramp",
                "pillar",
                "doorway",
            ],
        )?
        .as_deref()
    {
        Some("wedge") => hp_geom::Primitive::Wedge,
        Some("cylinder") => hp_geom::Primitive::Cylinder,
        Some("spike") => hp_geom::Primitive::Spike,
        Some("sphere") => hp_geom::Primitive::Sphere,
        Some("arch") => hp_geom::Primitive::Arch,
        Some("torus") => hp_geom::Primitive::Torus,
        Some("stairs") => hp_geom::Primitive::Stairs,
        Some("spiral_stairs") => hp_geom::Primitive::SpiralStairs,
        Some("ramp") => hp_geom::Primitive::Ramp,
        Some("pillar") => hp_geom::Primitive::Pillar,
        Some("doorway") => hp_geom::Primitive::Doorway,
        _ => hp_geom::Primitive::Block,
    };
    let mut bounds = hp_geom::Aabb::new(a.vec3("min")?, a.vec3("max")?);
    if let Some(d) = opt_axis(a, "depth_from_selection")? {
        let (lo, hi) = hp_tools_create::new_depth_range(state, state.runtime.brush.last_created, d);
        bounds.min[d] = lo;
        bounds.max[d] = hi;
    }
    let size = bounds.size();
    if size.min_element() < 1.0 || size.max_element() > 65536.0 {
        return Err("the box must be 1 to 65536 units on every axis".to_owned());
    }
    let material = opt_material(state, a, "material")?;
    let d = hp_geom::PrimitiveParams::default();
    let max_sides = u64::from(hp_geom::MAX_PRIMITIVE_SIDES);
    let rise = a.opt_f64("torus_rise", -16384.0, 16384.0)?.unwrap_or(0.0);
    let torus_arc = a.opt_f64("torus_arc", 1.0, hp_geom::MAX_SPIRAL_ARC)?;
    if rise == 0.0 && torus_arc.is_some_and(|t| t > 360.0) {
        return Err("a torus arc over 360 degrees needs torus_rise (a spiral)".to_owned());
    }
    let class = a.opt_str("entity_class", MAX_NAME)?.map(|c| c.trim().to_owned());
    if let Some(c) = &class {
        check_class(c)?;
    }
    let params = hp_geom::PrimitiveParams {
        kind,
        bounds,
        material: material.clone(),
        sides: a.opt_u64("sides", 3, max_sides)?.map_or(8, |n| n as u32),
        arch_wall_width: a.opt_f64("arch_wall_width", 1.0, 65536.0)?.unwrap_or(d.arch_wall_width),
        arch_arc_degrees: a.opt_f64("arch_arc", 1.0, 360.0)?.unwrap_or(d.arch_arc_degrees),
        arch_start_angle: a.opt_f64("arch_start_angle", -360.0, 360.0)?.unwrap_or(d.arch_start_angle),
        arch_add_height: a.opt_f64("arch_add_height", -65536.0, 65536.0)?.unwrap_or(d.arch_add_height),
        torus_ring_width: a.opt_f64("torus_ring_width", 1.0, 65536.0)?.unwrap_or(d.torus_ring_width),
        torus_tube_sides: a.opt_u64("torus_tube_sides", 3, max_sides)?.map_or(d.torus_tube_sides, |n| n as u32),
        torus_rotation_arc: torus_arc.unwrap_or(d.torus_rotation_arc),
        torus_rotation_start: a.opt_f64("torus_start_angle", -360.0, 360.0)?.unwrap_or(d.torus_rotation_start),
        torus_rotation_height: rise,
        stair_steps: a
            .opt_u64("stair_steps", 1, u64::from(hp_geom::MAX_STAIR_STEPS))?
            .map_or(d.stair_steps, |n| n as u32),
        stair_step_height: a.opt_f64("stair_step_height", 0.0, 1024.0)?.unwrap_or(d.stair_step_height),
        stair_open: a.opt_bool("stair_open")?.unwrap_or(d.stair_open),
        spiral_inner: a.opt_f64("spiral_inner", 0.0, 65536.0)?.unwrap_or(d.spiral_inner),
        spiral_angle: a
            .opt_f64("spiral_angle", -hp_geom::MAX_STAIR_ANGLE, hp_geom::MAX_STAIR_ANGLE)?
            .unwrap_or(d.spiral_angle),
        spiral_start: a.opt_f64("spiral_start", -360.0, 360.0)?.unwrap_or(d.spiral_start),
        ramp_angle: a.opt_f64("ramp_angle", 1.0, 89.0)?.unwrap_or(d.ramp_angle),
        pillar_base: a.opt_f64("pillar_base", 0.0, 65536.0)?.unwrap_or(d.pillar_base),
        pillar_capital: a.opt_f64("pillar_capital", 0.0, 65536.0)?.unwrap_or(d.pillar_capital),
        pillar_inset: a.opt_f64("pillar_inset", 0.0, 65536.0)?.unwrap_or(d.pillar_inset),
        door_width: a.opt_f64("door_width", 0.0, 65536.0)?.unwrap_or(d.door_width),
        door_height: a.opt_f64("door_height", 0.0, 65536.0)?.unwrap_or(d.door_height),
        door_thickness: a.opt_f64("door_thickness", 0.0, 65536.0)?.unwrap_or(d.door_thickness),
        door_arch: a.opt_bool("door_arch")?.unwrap_or(d.door_arch),
        door_arch_radius: a.opt_f64("door_arch_radius", 0.0, 65536.0)?.unwrap_or(d.door_arch_radius),
        door_arch_segments: a
            .opt_u64("door_arch_segments", 2, u64::from(hp_geom::MAX_DOOR_ARCH_SEGMENTS))?
            .map_or(d.door_arch_segments, |n| n as u32),
        direction: a
            .opt_enum("direction", &["+y", "-y", "+x", "-x", "longest"])?
            .and_then(|v| hp_geom::RiseDirection::from_id(&v))
            .unwrap_or(d.direction),
        up_axis: opt_axis(a, "up")?.unwrap_or(2),
        up_flip: a.opt_bool("upside_down")?.unwrap_or(false),
        tex_scale: a.opt_f64("tex_scale", 0.01, 64.0)?.unwrap_or_else(|| {
            let s = state.settings.edit.default_texture_scale;
            if s.is_finite() && s.abs() > 1e-6 { s } else { d.tex_scale }
        }),
        ..d
    };
    let (ids, entity) = edit(state, label, |doc| {
        let (ids, entity) = match &class {
            Some(class) => {
                let mut next = doc.peek_next_side_id();
                let pieces = hp_geom::make_primitive(&params, &mut || {
                    let id = next;
                    next = next.saturating_add(1);
                    id
                })?;
                let id = doc.create_brush_entity(class, &[], pieces)?;
                (doc.scene().entity_solid_ids(id), Some(id))
            }
            None => (doc.create_primitive(&params, Owner::World)?, None),
        };
        let objs: Vec<ObjRef> = match entity {
            Some(e) => vec![ObjRef::Entity(e)],
            None => ids.iter().map(|id| ObjRef::Solid(*id)).collect(),
        };
        if group && entity.is_none() {
            doc.group(&objs)?;
        }
        doc.select(&objs, SelectOp::Replace);
        Ok((ids, entity))
    })?;
    state.runtime.brush.last_created = Some(bounds);
    Ok(Made { ids, entity, bounds, material })
}

fn create_polygon_brush(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let view = a.opt_enum("view", &["top", "front", "side"])?.ok_or("view is required")?;
    let list = a.get("points").and_then(Value::as_array).filter(|l| (3..=64).contains(&l.len()));
    let list = list.ok_or("points must be 3 to 64 [a, b] points")?;
    let mut outline = Vec::with_capacity(list.len());
    for p in list {
        let ab: Vec<f64> = p
            .as_array()
            .filter(|x| x.len() == 2)
            .into_iter()
            .flatten()
            .filter_map(Value::as_f64)
            .filter(|f| f.is_finite() && f.abs() <= 131072.0)
            .collect();
        if ab.len() != 2 {
            return Err("points must contain [a, b] points with numbers from -131072 to 131072".to_owned());
        }
        outline.push(glam::DVec2::new(ab[0], ab[1]));
    }
    if !hp_tools_create::is_convex_outline(&outline) {
        return Err("the outline must be convex, without repeated or aligned points".to_owned());
    }
    let depth = opt_vec2(a, "depth", -131072.0, 131072.0)?.ok_or("depth is required")?;
    let (lo, hi) = (depth.x.min(depth.y), depth.x.max(depth.y));
    if hi - lo < 1.0 {
        return Err("the depth range must be at least 1 unit".to_owned());
    }
    let to3 = |p: glam::DVec2, d: f64| match view.as_str() {
        "top" => DVec3::new(p.x, p.y, d),
        "front" => DVec3::new(d, p.x, p.y),
        _ => DVec3::new(p.x, d, p.y),
    };
    let points: Vec<DVec3> = outline.iter().flat_map(|p| [to3(*p, lo), to3(*p, hi)]).collect();
    let material = opt_material(state, a, "material")?;
    let params = hp_geom::PrimitiveParams {
        kind: hp_geom::Primitive::Block,
        bounds: hp_geom::Aabb::from_points(points.iter().copied()),
        material: material.clone(),
        ..Default::default()
    };
    let id = edit(state, "create_polygon_brush", |doc| {
        let ids = doc.create_primitive(&params, Owner::World)?;
        let id = *ids.first().ok_or(hp_editor::EditError::Nothing("no solid created"))?;
        doc.set_solid_vertices(id, &points)?;
        doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
        Ok(id)
    })?;
    Ok(ToolResult::json(json!({ "solid_id": id, "material": material })))
}

fn snap_brush_vertices(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let solids = target_solids(state, a, "solid_ids")?;
    let g = a.opt_f64("grid", 0.125, 4096.0)?.unwrap_or_else(|| grid(state));
    let (changed, skipped) = edit(state, "snap_brush_vertices", |doc| {
        let (mut changed, mut skipped) = (0usize, 0usize);
        for id in &solids {
            let Some((points, _)) = doc.solid_vertices(*id) else { continue };
            let snapped: Vec<DVec3> = points.iter().map(|p| (*p / g).round() * g).collect();
            if snapped == points {
                continue;
            }
            match doc.set_solid_vertices(*id, &snapped) {
                Ok(()) => changed += 1,
                Err(_) => skipped += 1,
            }
        }
        Ok((changed, skipped))
    })?;
    Ok(ToolResult::json(json!({ "solids_changed": changed, "solids_left": skipped, "grid": g })))
}

fn create_room(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let inner = hp_geom::Aabb::new(a.vec3("min")?, a.vec3("max")?);
    if inner.size().min_element() < 1.0 {
        return Err("the inside box must be at least 1 unit on every axis".to_owned());
    }
    let t = a.opt_f64("thickness", 1.0, 1024.0)?.unwrap_or(16.0);
    let outer = hp_geom::Aabb::new(inner.min - DVec3::splat(t), inner.max + DVec3::splat(t));
    if outer.size().max_element() > 65536.0 {
        return Err("the room is larger than 65536 units".to_owned());
    }
    let wall = opt_material(state, a, "material")?;
    let floor = match a.opt_str("floor_material", MAX_NAME)? {
        Some(m) => material_name(m)?,
        None => wall.clone(),
    };
    let ceiling = match a.opt_str("ceiling_material", MAX_NAME)? {
        Some(m) => material_name(m)?,
        None => wall.clone(),
    };
    let params = hp_geom::PrimitiveParams {
        kind: hp_geom::Primitive::Block,
        bounds: outer,
        material: wall.clone(),
        ..Default::default()
    };
    let walls = edit(state, "create_room", |doc| {
        let ids = doc.create_primitive(&params, Owner::World)?;
        let walls = doc.hollow(&ids, t)?;
        let mut floor_faces = Vec::new();
        let mut ceiling_faces = Vec::new();
        for id in &walls {
            let Some(n) = doc.scene().solid(*id) else { continue };
            for (i, s) in n.solid.sides.iter().enumerate() {
                let Some(p) = hp_geom::Plane::from_side(s) else { continue };
                let c = n
                    .geom
                    .faces
                    .iter()
                    .find(|f| f.side_index == i)
                    .map(|f| f.vertices.iter().copied().sum::<DVec3>() / f.vertices.len().max(1) as f64);
                let Some(c) = c else { continue };
                if p.normal.z > 0.99 && (c.z - inner.min.z).abs() < 1e-3 {
                    floor_faces.push(FaceRef::new(*id, i));
                } else if p.normal.z < -0.99 && (c.z - inner.max.z).abs() < 1e-3 {
                    ceiling_faces.push(FaceRef::new(*id, i));
                }
            }
        }
        if floor != wall && !floor_faces.is_empty() {
            doc.apply_material(&floor_faces, &floor)?;
        }
        if ceiling != wall && !ceiling_faces.is_empty() {
            doc.apply_material(&ceiling_faces, &ceiling)?;
        }
        Ok(walls)
    })?;
    Ok(ToolResult::json(json!({ "wall_ids": walls, "inside": bounds_json(Some(inner)) })))
}

fn create_stairs(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let b = hp_geom::Aabb::new(a.vec3("min")?, a.vec3("max")?);
    let n = a.opt_u64("steps", 2, 256)?.ok_or("steps is required")?;
    let dir = a.opt_enum("direction", &["+x", "-x", "+y", "-y"])?.ok_or("direction is required")?;
    let size = b.size();
    let axis = if dir.ends_with('x') { 0 } else { 1 };
    let (run, rise) = (size[axis] / n as f64, size.z / n as f64);
    if run < 1.0 || rise < 1.0 || size[1 - axis] < 1.0 {
        return Err("each step must be at least 1 unit deep, wide and high".to_owned());
    }
    let mut map = serde_json::Map::new();
    map.insert("shape".to_owned(), json!("stairs"));
    map.insert("stair_steps".to_owned(), json!(n));
    for key in ["min", "max", "direction", "material"] {
        if let Some(v) = a.get(key) {
            map.insert(key.to_owned(), v.clone());
        }
    }
    let made = make_shape(state, &Args { map: &map }, "create_stairs", true)?;
    Ok(ToolResult::json(json!({ "solid_ids": made.ids, "step_depth": run, "step_height": rise })))
}

fn make_hint_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = target_faces(state, a, "faces")?;
    let n = edit(state, "make_hint_brushes", |doc| doc.make_hint_brushes(&faces))?;
    Ok(ToolResult::json(json!({ "solids": n })))
}

fn apply_nodraw(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = target_faces(state, a, "faces")?;
    let n = edit(state, "apply_nodraw", |doc| doc.apply_nodraw(&faces))?;
    Ok(ToolResult::json(json!({ "faces": n })))
}

fn tie_to_entity(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let kv = a.keyvalues("keyvalues")?;
    if let Some(entity) = a.opt_u64("entity", 1, u64::from(u32::MAX))? {
        let entity = entity as u32;
        if a.get("entity_class").is_some() || a.get("each").is_some() || !kv.is_empty() {
            return Err("entity cannot be combined with entity_class, keyvalues or each".to_owned());
        }
        let is_brush_entity = !state.doc.scene().entity_solid_ids(entity).is_empty();
        if !is_brush_entity {
            return Err(format!("{entity} is not a brush entity"));
        }
        let n = edit(state, "tie_to_entity", |doc| doc.tie_to_existing(&objs, entity))?;
        return Ok(ToolResult::json(json!({ "entity_id": entity, "solids_moved": n })));
    }
    let class = a.opt_str("entity_class", MAX_NAME)?.map_or("func_detail", str::trim).to_owned();
    check_class(&class)?;
    let origin = hp_app::runtime::helpers::brush_entity_needs_origin(state.assets.fgd(), &class);
    if a.opt_bool("each")?.unwrap_or(false) {
        let ids = edit(state, "tie_to_entity", |doc| {
            let ids = doc.tie_each_to_entity(&objs, &class, &kv)?;
            if origin {
                doc.add_missing_origins(&ids)?;
            }
            Ok(ids)
        })?;
        return Ok(ToolResult::json(json!({ "entity_ids": ids, "classname": class })));
    }
    let id = edit(state, "tie_to_entity", |doc| {
        let id = doc.tie_to_entity(&objs, &class, &kv)?;
        if origin {
            doc.add_missing_origins(&[id])?;
        }
        Ok(id)
    })?;
    Ok(ToolResult::json(json!({ "entity_id": id, "classname": class })))
}

fn move_to_world(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let n = edit(state, "move_to_world", |doc| doc.move_to_world(&objs))?;
    Ok(ToolResult::json(json!({ "solids_moved": n })))
}

fn prop_to_brushes(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    use hp_app::settings::UnpropShape;
    let prop = match a.opt_u64("id", 1, u64::from(u32::MAX))? {
        Some(id) => id as u32,
        None => hp_app::actions::selected_prop(state).ok_or("select a prop or give its id")?,
    };
    let shape = match a.opt_str("shape", MAX_NAME)? {
        None | Some("auto") => UnpropShape::Auto,
        Some("collision") => UnpropShape::Collision,
        Some("render") => UnpropShape::Render,
        Some(other) => return Err(format!("unknown shape {other}: auto, collision or render")),
    };
    let detail = match a.opt_str("target", MAX_NAME)? {
        None | Some("detail") => true,
        Some("world") => false,
        Some(other) => return Err(format!("unknown target {other}: detail or world")),
    };
    let keep = a.opt_bool("keep_prop")?.unwrap_or(false);
    let source = hp_ui_mapping::propper::prop_source(state, prop)?;
    let made = hp_ui_mapping::propper::build_brushes(&source, shape)?;
    let solids = made.brushes.solids.clone();
    let ids = edit(state, "prop_to_brushes", |doc| doc.replace_prop_with_brushes(prop, solids, detail, keep))?;
    Ok(ToolResult::json(json!({
        "model": source.model,
        "solid_ids": ids,
        "from_collision_model": made.from_collision,
        "textured_faces": made.brushes.textured_faces,
        "nodraw_faces": made.brushes.nodraw_faces,
        "approximate_brushes": made.approximate,
        "skipped_pieces": made.brushes.skipped,
        "prop_kept": keep,
    })))
}

fn get_faces(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let objs = targets(state, a)?;
    let with_vertices = a.opt_bool("vertices")?.unwrap_or(true);
    let doc = &state.doc;
    let solids = doc.scene().expand_solids(&objs);
    if solids.len() > 1000 {
        return Err(format!("{} solids: ask for at most 1000 at once", solids.len()));
    }
    let mut out = Vec::new();
    for id in solids {
        let Some(n) = doc.scene().solid(id) else { continue };
        let faces: Vec<Value> = n
            .solid
            .sides
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let normal = hp_geom::Plane::from_side(s).map(|p| v3(p.normal));
                let mut f = json!({
                    "side": i,
                    "material": s.material,
                    "normal": normal,
                    "u_axis": v3(s.uaxis.axis), "u_scale": s.uaxis.scale, "u_shift": s.uaxis.shift,
                    "v_axis": v3(s.vaxis.axis), "v_scale": s.vaxis.scale, "v_shift": s.vaxis.shift,
                    "rotation": s.rotation,
                    "lightmap_scale": s.lightmap_scale,
                    "smoothing_groups": s.smoothing_groups,
                    "displacement_power": s.disp.as_ref().map(|d| d.power),
                });
                if with_vertices {
                    f["vertices"] = doc
                        .face_polygon(FaceRef::new(id, i))
                        .map_or(Value::Null, |p| Value::Array(p.iter().map(|v| v3(*v)).collect()));
                }
                f
            })
            .collect();
        out.push(json!({ "solid": id, "faces": faces }));
    }
    Ok(ToolResult::json(json!({ "solids": out })))
}

fn set_face_texture(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = target_faces(state, a, "faces")?;
    let dir = |k: &str| -> R<Option<DVec3>> { opt_dir(a, k) };
    let scale = |k: &str| -> R<Option<f64>> {
        match a.opt_f64(k, -1024.0, 1024.0)? {
            Some(v) if v.abs() < 1e-4 => Err(format!("{k} must not be 0")),
            v => Ok(v),
        }
    };
    let fe = FaceEdit {
        material: a.opt_str("material", MAX_NAME)?.map(material_name).transpose()?,
        align: match a.opt_enum("align", &["world", "face"])?.as_deref() {
            Some("world") => Some(hp_geom::TextureAlign::World),
            Some(_) => Some(hp_geom::TextureAlign::Face),
            None => None,
        },
        u_axis: dir("u_axis")?,
        v_axis: dir("v_axis")?,
        rotation: a.opt_f64("rotation", -360.0, 360.0)?,
        rotate_by: a.opt_f64("rotate_by", -360.0, 360.0)?,
        rotation_value: None,
        u_scale: scale("u_scale")?,
        v_scale: scale("v_scale")?,
        scale_by: opt_vec2(a, "scale_by", -1024.0, 1024.0)?,
        u_shift: a.opt_f64("u_shift", -65536.0, 65536.0)?,
        v_shift: a.opt_f64("v_shift", -65536.0, 65536.0)?,
        shift_by: opt_vec2(a, "shift_by", -65536.0, 65536.0)?,
        lightmap_scale: a.opt_u64("lightmap_scale", 1, 1024)?.map(|n| n as i32),
        smoothing_groups: a.opt_u64("smoothing_groups", 0, u64::from(u32::MAX))?.map(|n| n as u32),
    };
    if fe.scale_by.is_some_and(|s| s.x.abs() < 1e-4 || s.y.abs() < 1e-4) {
        return Err("scale_by must not contain 0".to_owned());
    }
    let n = edit(state, "set_face_texture", |doc| doc.edit_faces(&faces, &fe))?;
    Ok(ToolResult::json(json!({ "faces": n })))
}

fn justify_texture(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = target_faces(state, a, "faces")?;
    let mode = match a.opt_enum("mode", &["fit", "left", "right", "top", "bottom", "center"])?.as_deref() {
        Some("fit") => hp_geom::Justify::Fit,
        Some("left") => hp_geom::Justify::Left,
        Some("right") => hp_geom::Justify::Right,
        Some("top") => hp_geom::Justify::Top,
        Some("bottom") => hp_geom::Justify::Bottom,
        _ => hp_geom::Justify::Center,
    };
    let one = a.opt_bool("treat_as_one")?.unwrap_or(false);
    let sizes = texture_sizes(state, &faces);
    let size = |m: &str| sizes.get(&m.to_ascii_lowercase()).copied();
    let n = edit(state, "justify_texture", |doc| doc.justify_faces(&faces, mode, one, &size))?;
    Ok(ToolResult::json(json!({ "faces": n })))
}

fn fit_texture(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = target_faces(state, a, "faces")?;
    let ru = a.opt_f64("repeat_u", 0.01, 1024.0)?.unwrap_or(1.0);
    let rv = a.opt_f64("repeat_v", 0.01, 1024.0)?.unwrap_or(1.0);
    let one = a.opt_bool("treat_as_one")?.unwrap_or(false);
    let sizes = texture_sizes(state, &faces);
    let size = |m: &str| sizes.get(&m.to_ascii_lowercase()).copied();
    let n = edit(state, "fit_texture", |doc| doc.fit_faces(&faces, ru, rv, one, &size))?;
    Ok(ToolResult::json(json!({ "faces": n })))
}

fn align_texture_to_view(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let faces = target_faces(state, a, "faces")?;
    let (right, up) = match (a.opt_enum("view", &["top", "front", "side"])?, opt_dir(a, "right")?, opt_dir(a, "up")?) {
        (Some(v), None, None) => match v.as_str() {
            "top" => (DVec3::X, DVec3::Y),
            "front" => (DVec3::Y, DVec3::Z),
            _ => (DVec3::X, DVec3::Z),
        },
        (None, Some(r), Some(u)) => {
            if r.cross(u).length() < 1e-6 {
                return Err("right and up must not be parallel".to_owned());
            }
            (r, u)
        }
        _ => return Err("give view, or right and up".to_owned()),
    };
    let n = edit(state, "align_texture_to_view", |doc| doc.align_faces_to_view(&faces, right, up))?;
    Ok(ToolResult::json(json!({ "faces": n })))
}

fn wrap_texture(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let src = opt_faces(&state.doc, a, "source")?.unwrap_or_default();
    let [src] = src.as_slice() else { return Err("source must be exactly one face".to_owned()) };
    let src = *src;
    let dst: Vec<FaceRef> = target_faces(state, a, "faces")?.into_iter().filter(|f| *f != src).collect();
    if dst.is_empty() {
        return Err("faces must name faces other than the source".to_owned());
    }
    let n = edit(state, "wrap_texture", |doc| doc.wrap_texture(src, &dst))?;
    Ok(ToolResult::json(json!({ "faces": n })))
}

fn replace_materials(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let find = a.str("find", MAX_NAME)?.trim().replace('\\', "/").to_ascii_lowercase();
    if find.is_empty() {
        return Err("find must not be empty".to_owned());
    }
    let replace = material_name(a.str("replace", MAX_NAME)?)?;
    let scope = match a.opt_enum("scope", &["all", "selection", "ids"])?.as_deref() {
        Some("selection") => ReplaceScope::Selection,
        Some("ids") => ReplaceScope::Objects(targets(state, a)?),
        _ => {
            if a.get("ids").is_some() || a.get("solid_ids").is_some() {
                return Err("ids need scope \"ids\"".to_owned());
            }
            ReplaceScope::All
        }
    };
    let n = edit(state, "replace_materials", |doc| doc.replace_materials(&find, &replace, &scope))?;
    Ok(ToolResult::json(json!({ "faces": n, "material": replace })))
}

fn list_used_materials(state: &mut AppState, a: &Args<'_>) -> R<ToolResult> {
    let used = state.doc.used_materials();
    let (items, total) = page(used, a)?;
    let list: Vec<Value> = items.into_iter().map(|(m, n)| json!({ "material": m, "faces": n })).collect();
    Ok(ToolResult::json(json!({ "total": total, "materials": list })))
}

#[cfg(test)]
mod tests {
    use hp_editor::{ObjRef, SelectOp};
    use serde_json::json;

    use super::super::semantic_tests::{err, ok};

    fn solid_count(state: &hp_app::AppState) -> usize {
        state.doc.stats().solids
    }

    #[test]
    fn random_rotate_objects_turns_entities_with_angles() {
        let mut state = hp_app::testing::test_state();
        let r =
            ok(&mut state, "random_rotate_objects", json!({ "ids": [4, 3], "min_yaw": 90, "max_yaw": 90, "seed": 5 }));
        assert_eq!((r["rotated"].as_u64(), r["skipped"].as_u64()), (Some(1), Some(1)));
        let a = state.doc.scene().entity(4).and_then(|n| n.entity.angles()).expect("angles");
        assert_eq!(a.y, 180.0);
        assert_eq!(state.doc.undo_label(), Some("MCP: random_rotate_objects"));
        let r = ok(&mut state, "random_rotate_objects", json!({ "ids": [4], "snap": 90, "seed": 9 }));
        assert_eq!(r["rotated"], 1);
        let y = state.doc.scene().entity(4).and_then(|n| n.entity.angles()).expect("angles").y;
        assert_eq!(y % 90.0, 0.0);
        err(&mut state, "random_rotate_objects", json!({ "ids": [4], "min_yaw": 90, "max_yaw": 10 }));
        err(&mut state, "random_rotate_objects", json!({ "ids": [4], "tilt": 90 }));
    }

    #[test]
    fn duplicate_transform_flip_resize() {
        let mut s = hp_app::testing::test_state();
        let n = solid_count(&s);
        let r = ok(&mut s, "duplicate_objects", json!({ "solid_ids": [3], "offset": [0, 0, 256] }));
        assert_eq!(r["copies"].as_array().unwrap().len(), 1);
        assert_eq!(solid_count(&s), n + 1);
        assert_eq!(s.doc.undo_label(), Some("MCP: duplicate_objects"));
        let r = ok(&mut s, "array_duplicate", json!({ "solid_ids": [3], "count": 3, "offset": [0, 0, 200] }));
        assert_eq!(r["copies"].as_array().unwrap().len(), 3);
        assert_eq!(solid_count(&s), n + 4);
        err(&mut s, "array_duplicate", json!({ "solid_ids": [3], "count": 2 }));
        let before = solid_count(&s);
        let r = ok(&mut s, "array_duplicate", json!({ "solid_ids": [3], "axes": ["x", "y"], "count": 1, "gap": 8 }));
        assert_eq!(r["copies"].as_array().unwrap().len(), 3);
        assert_eq!(solid_count(&s), before + 3);
        assert!(r["steps"][0].as_f64().unwrap() >= 520.0);
        assert_eq!(s.doc.undo_label(), Some("MCP: array_duplicate"));
        let r = ok(
            &mut s,
            "array_duplicate",
            json!({ "solid_ids": [3], "counts": [0, 0, 2], "gaps": [0, 0, 5], "snap": false }),
        );
        assert_eq!(r["steps"][2], json!(133.0));
        err(&mut s, "array_duplicate", json!({ "solid_ids": [3], "axes": ["w"], "count": 1 }));
        err(&mut s, "array_duplicate", json!({ "solid_ids": [3], "axes": ["x"] }));

        let r = ok(
            &mut s,
            "transform_objects",
            json!({ "solid_ids": [2], "translate": [0, 0, -100], "scale": [0.5, 0.5, 1] }),
        );
        assert_eq!(r["bounds"]["min"], json!([-128.0, -128.0, -116.0]));
        ok(&mut s, "resize_objects", json!({ "solid_ids": [2], "min": [0, 0, 0], "max": [64, 64, 8] }));
        let b = s.doc.scene().objects_bounds(&[ObjRef::Solid(2)], &|_| None).unwrap();
        assert_eq!((b.min.to_array(), b.max.to_array()), ([0.0, 0.0, 0.0], [64.0, 64.0, 8.0]));
        ok(&mut s, "flip_objects", json!({ "solid_ids": [2], "axis": "x", "center": [0, 0, 0] }));
        let b = s.doc.scene().objects_bounds(&[ObjRef::Solid(2)], &|_| None).unwrap();
        assert_eq!(b.min.x, -64.0);
        err(&mut s, "transform_objects", json!({ "solid_ids": [2], "scale": [0, 1, 1] }));
        s.doc.select(&[ObjRef::Solid(2)], SelectOp::Replace);
        ok(&mut s, "snap_objects_to_grid", json!({ "grid": 128 }));
        let b = s.doc.scene().objects_bounds(&[ObjRef::Solid(2)], &|_| None).unwrap();
        assert_eq!(b.min.x % 128.0, 0.0);
        s.doc.clear_selection();
        err(&mut s, "flip_objects", json!({ "axis": "x" }));
    }

    #[test]
    fn align_and_drop() {
        let mut s = hp_app::testing::test_state();
        ok(&mut s, "align_objects", json!({ "solid_ids": [3], "axis": "z", "mode": "min", "value": 32 }));
        let b = s.doc.scene().objects_bounds(&[ObjRef::Solid(3)], &|_| None).unwrap();
        assert_eq!(b.min.z, 32.0);
        ok(&mut s, "align_objects", json!({ "solid_ids": [3], "axis": "z", "mode": "max", "reference_id": 2 }));
        let b = s.doc.scene().objects_bounds(&[ObjRef::Solid(3)], &|_| None).unwrap();
        assert_eq!(b.max.z, 0.0);
        err(
            &mut s,
            "align_objects",
            json!({ "solid_ids": [3], "axis": "z", "mode": "max", "reference_id": 2, "value": 1 }),
        );
        ok(&mut s, "transform_objects", json!({ "ids": [5], "translate": [0, 0, 500] }));
        let r = ok(&mut s, "drop_to_surface", json!({ "ids": [5] }));
        assert_eq!(r["moved"], 1);
    }

    #[test]
    fn csg_tools() {
        let mut s = hp_app::testing::test_state();
        let r =
            ok(&mut s, "clip_brushes", json!({ "solid_ids": [2], "normal": [1, 0, 0], "distance": 0, "keep": "both" }));
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 2);
        err(&mut s, "clip_brushes", json!({ "solid_ids": [2] }));
        err(&mut s, "clip_brushes", json!({ "solid_ids": [3], "points": [[0, 0, 0], [1, 0, 0], [2, 0, 0]] }));
        let r = ok(
            &mut s,
            "clip_brushes",
            json!({ "solid_ids": [3], "point": [0, 0, 64], "normal": [0, 0, 2], "keep": "back", "cap_material": "tools/toolsnodraw" }),
        );
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 1);
        let wall = s.doc.scene().solid(3).unwrap();
        assert_eq!((wall.bounds.min.z, wall.bounds.max.z), (0.0, 64.0));
        assert_eq!(wall.solid.sides.iter().filter(|x| x.material.eq_ignore_ascii_case("tools/toolsnodraw")).count(), 1);
        err(
            &mut s,
            "clip_brushes",
            json!({ "solid_ids": [3], "point": [0, 0, 32], "normal": [0, 0, 1], "distance": 32 }),
        );
        err(&mut s, "clip_brushes", json!({ "solid_ids": [3], "normal": [0, 0, 1] }));
        let r = ok(&mut s, "clip_brushes", json!({ "solid_ids": [3], "point": [0, 0, 63.9], "normal": [0, 0, 1] }));
        assert!(r["solid_ids"].as_array().unwrap().is_empty(), "no sliver");
        let r = ok(&mut s, "clip_brushes", json!({ "solid_ids": [7], "point": [96, 0, 0], "normal": [1, 0, 0] }));
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 2);
        assert!(s.doc.scene().solids().values().filter(|n| n.owner == hp_editor::Owner::Entity(6)).count() == 2);
        let mut s = hp_app::testing::test_state();
        let b = ok(&mut s, "create_primitive", json!({ "min": [-32, -32, -64], "max": [32, 32, 64] }));
        let carver = b["solid_ids"][0].as_u64().unwrap();
        let r = ok(&mut s, "carve", json!({ "solid_ids": [carver] }));
        assert_eq!(r["carved"], 1);
        assert_eq!(s.doc.undo_label(), Some("MCP: carve"));
        let r = ok(&mut s, "hollow", json!({ "solid_ids": [3], "thickness": 4 }));
        assert!(r["wall_ids"].as_array().unwrap().len() >= 6);
        err(&mut s, "hollow", json!({ "solid_ids": [carver], "thickness": 0.1 }));
        let mut s = hp_app::testing::test_state();
        let a =
            ok(&mut s, "create_primitive", json!({ "min": [0, 0, 300], "max": [64, 64, 364] }))["solid_ids"][0].clone();
        let b = ok(&mut s, "create_primitive", json!({ "min": [64, 0, 300], "max": [128, 64, 364] }))["solid_ids"][0]
            .clone();
        let r = ok(&mut s, "merge_brushes", json!({ "solid_ids": [a, b] }));
        assert_eq!(r["merged"], 2);
        err(&mut s, "merge_brushes", json!({ "solid_ids": [2, 3] }));
        ok(&mut s, "merge_brushes", json!({ "solid_ids": [2, 3], "convex_hull": true }));
    }

    #[test]
    fn intersect_and_union_brushes() {
        let mut s = hp_app::testing::test_state();
        let mk = |s: &mut _, min: [i32; 3], max: [i32; 3]| {
            ok(s, "create_primitive", json!({ "min": min, "max": max }))["solid_ids"][0].clone()
        };
        let a = mk(&mut s, [0, 0, 900], [64, 64, 964]);
        let b = mk(&mut s, [32, 0, 900], [96, 64, 964]);
        let r = ok(&mut s, "union_brushes", json!({ "solid_ids": [a, b] }));
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 1);
        assert_eq!(s.doc.undo_label(), Some("MCP: union_brushes"));
        let c = mk(&mut s, [0, 0, 1200], [64, 64, 1264]);
        let d = mk(&mut s, [32, 0, 1200], [96, 64, 1264]);
        let r = ok(&mut s, "intersect_brushes", json!({ "solid_ids": [c, d] }));
        assert_eq!(r["solid_id"], c);
        err(&mut s, "intersect_brushes", json!({ "solid_ids": [c] }));
    }

    #[test]
    fn bevel_round_cove_and_fill() {
        let mut s = hp_app::testing::test_state();
        let edges = ok(&mut s, "get_brush_edges", json!({ "solid": 3 }));
        let list = edges["edges"].as_array().unwrap();
        assert_eq!(list.len(), 12);
        let top = list
            .iter()
            .position(|e| e["a"][1] == 256.0 && e["b"][1] == 256.0 && e["a"][2] == 128.0 && e["b"][2] == 128.0)
            .unwrap();
        let dry = ok(&mut s, "bevel_edges", json!({ "solid": 3, "edge": top, "radius": 100, "dry_run": true }));
        assert_eq!(dry["max_radius"], 16.0);
        assert_eq!(dry["radius"], 16.0);
        assert_eq!(dry["kinds"], json!(["round"]));
        assert!(s.doc.undo_label().is_none_or(|l| l != "MCP: bevel_edges"), "a dry run changes nothing");
        let r = ok(&mut s, "bevel_edges", json!({ "solid": 3, "edge": top, "radius": 8, "segments": 6 }));
        assert_eq!(r["solid_ids"], json!([3]));
        assert_eq!((r["radius"].as_f64(), r["segments"].as_u64()), (Some(8.0), Some(6)));
        assert_eq!(s.doc.undo_label(), Some("MCP: bevel_edges"));
        assert_eq!(s.doc.scene().solid(3).unwrap().solid.sides.len(), 12);
        assert!(s.doc.undo());
        let corner = json!([[-256, 256, 0], [256, 256, 0]]);
        let r = ok(
            &mut s,
            "bevel_edges",
            json!({ "solid": 3, "edges": [corner], "radius": 32, "segments": 4, "inverse": true, "material": "dev/dev_blendmeasure" }),
        );
        assert_eq!(r["kinds"], json!(["fill"]), "the wall stands on the floor");
        let ids = r["solid_ids"].as_array().unwrap();
        assert!(ids.len() >= 2);
        let curved = ids
            .iter()
            .flat_map(|id| s.doc.scene().solid(id.as_u64().unwrap() as u32).unwrap().solid.sides.clone())
            .filter(|side| side.material == "dev/dev_blendmeasure")
            .count();
        assert_eq!(curved, 4);
        assert!(s.doc.undo());
        let r = ok(
            &mut s,
            "bevel_edges",
            json!({ "edges": [{ "solid": 3, "a": [-256, 272, 128], "b": [256, 272, 128] }, { "solid": 2, "edge": 0 }], "radius": 4, "inverse": true }),
        );
        assert!(r["kinds"].as_array().unwrap().len() == 2);
        for id in r["solid_ids"].as_array().unwrap() {
            assert!(s.doc.scene().solid(id.as_u64().unwrap() as u32).unwrap().geom.valid);
        }
        err(&mut s, "bevel_edges", json!({ "solid": 3 }));
        err(&mut s, "bevel_edges", json!({ "edge": 0 }));
        err(&mut s, "bevel_edges", json!({ "solid": 3, "edge": 99 }));
        err(&mut s, "bevel_edges", json!({ "solid": 3, "edge": [[0, 0, 0], [1, 1, 1]] }));
        err(&mut s, "bevel_edges", json!({ "solid": 3, "edge": top, "radius": 0 }));
        err(&mut s, "bevel_edges", json!({ "solid": 3, "edge": top, "segments": 40 }));
        err(&mut s, "bevel_edges", json!({ "solid": 3, "edge": "top" }));
        err(&mut s, "get_brush_edges", json!({ "solid": 999 }));
    }

    #[test]
    fn inset_a_face_keeps_the_volume() {
        let mut s = hp_app::testing::test_state();
        let before = s.doc.scene().solid(3).unwrap().geom.volume();
        let r = ok(&mut s, "inset_face", json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": 4 }));
        assert_eq!(r["brushes"], 5);
        assert_eq!(r["solid_ids"][0], 3);
        assert_eq!(s.doc.undo_label(), Some("MCP: inset_face"));
        let vol: f64 = r["solid_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| s.doc.scene().solid(id.as_u64().unwrap() as u32).unwrap().geom.volume())
            .sum();
        assert!((vol - before).abs() < 1e-6);
        assert!(s.doc.undo());
        let r = ok(
            &mut s,
            "inset_face",
            json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": 5, "depth": 3, "snap": true }),
        );
        assert_eq!(r["depth"], 0.0);
        err(&mut s, "inset_face", json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": 1000 }));
        err(&mut s, "inset_face", json!({ "faces": [{ "solid": 3, "side": 99 }] }));
        let dry =
            ok(&mut s, "inset_face", json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": 4, "dry_run": true }));
        assert_eq!(dry["brushes"], 5);
        assert_eq!(dry["dry_run"], true);
        let two = json!({ "faces": [{ "solid": 3, "side": 5 }, { "solid": 3, "side": 4 }], "distance": 4 });
        let r = ok(&mut s, "inset_face", two);
        assert!(r["brushes"].as_u64().unwrap() > 5);
    }

    #[test]
    fn extrude_faces_make_new_aligned_brushes() {
        let mut s = hp_app::testing::test_state();
        let before = s.doc.scene().solids().len();
        let dry = ok(
            &mut s,
            "extrude_faces",
            json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": 20, "dry_run": true }),
        );
        assert_eq!(dry["brushes"], 1);
        assert_eq!(s.doc.scene().solids().len(), before);
        let r =
            ok(&mut s, "extrude_faces", json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": 21, "snap": true }));
        assert_eq!(r["brushes"], 1);
        assert_eq!(r["faces"][0]["distance"], 16.0);
        assert_eq!(s.doc.scene().solids().len(), before + 1);
        assert_eq!(s.doc.undo_label(), Some("MCP: extrude_faces"));
        assert!(s.doc.undo());
        let r = ok(
            &mut s,
            "extrude_faces",
            json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": 21, "alt_grid": true }),
        );
        assert_eq!(r["faces"][0]["distance"], 24.0);
        assert!(s.doc.undo());
        let r = ok(
            &mut s,
            "extrude_faces",
            json!({ "faces": [{ "solid": 3, "side": 5 }, { "solid": 3, "side": 4 }], "distance": 8 }),
        );
        assert_eq!(r["brushes"], 2);
        err(&mut s, "extrude_faces", json!({ "faces": [{ "solid": 3, "side": 99 }], "distance": 8 }));
        err(&mut s, "extrude_faces", json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": 0 }));
    }

    #[test]
    fn extrude_faces_with_a_negative_distance_carve_the_brush() {
        let mut s = hp_app::testing::test_state();
        let before = s.doc.scene().solids().len();
        let volume = s.doc.scene().solid(3).unwrap().geom.volume();
        let dry = ok(
            &mut s,
            "extrude_faces",
            json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": -2, "dry_run": true }),
        );
        assert_eq!(dry["brushes"], 1);
        assert_eq!(s.doc.scene().solid(3).unwrap().geom.volume(), volume);
        let r = ok(&mut s, "extrude_faces", json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": -2 }));
        assert_eq!(r["solid_ids"], json!([3]));
        assert_eq!(r["faces"][0]["distance"], -2.0);
        assert_eq!(s.doc.scene().solids().len(), before);
        assert!(s.doc.scene().solid(3).unwrap().geom.volume() < volume);
        assert_eq!(s.doc.undo_label(), Some("MCP: extrude_faces"));
        assert!(s.doc.undo());
        assert_eq!(s.doc.scene().solid(3).unwrap().geom.volume(), volume);
        err(&mut s, "extrude_faces", json!({ "faces": [{ "solid": 3, "side": 5 }], "distance": -16000 }));
    }

    #[test]
    fn region_inset_and_individual_are_a_choice() {
        let mut s = hp_app::testing::test_state();
        let faces = json!([{ "solid": 3, "side": 5 }, { "solid": 3, "side": 4 }]);
        let shared = ok(&mut s, "inset_face", json!({ "faces": faces, "distance": 4, "dry_run": true }));
        assert_eq!(shared["individual"], false);
        let alone =
            ok(&mut s, "inset_face", json!({ "faces": faces, "distance": 4, "individual": true, "dry_run": true }));
        assert_eq!(alone["individual"], true);
        assert_eq!(alone["brushes"], shared["brushes"]);
    }

    #[test]
    fn edges_and_vertices_are_selected_by_number() {
        let mut s = hp_app::testing::test_state();
        let r = ok(&mut s, "select_elements", json!({ "element": "edge", "solid": 3, "indices": [0, 1] }));
        assert_eq!(r["element"], "edge");
        assert_eq!(r["edge_count"], 2);
        assert_eq!(r["vertex_count"], 0);
        let r = ok(&mut s, "select_elements", json!({ "element": "edge", "solid": 3, "indices": [1], "op": "remove" }));
        assert_eq!(r["edge_count"], 1);
        let r = ok(&mut s, "select_elements", json!({ "element": "vertex", "solid": 3, "indices": [0, 2, 2] }));
        assert_eq!(r["element"], "vertex");
        assert_eq!(r["vertex_count"], 2);
        assert_eq!(r["edge_count"], 0);
        let sel = ok(&mut s, "get_selection", json!({}));
        assert_eq!(sel["vertex_count"], 2);
        assert_eq!(sel["element"], "vertex");
        err(&mut s, "select_elements", json!({ "element": "edge", "solid": 3, "indices": [9999] }));
        err(&mut s, "select_elements", json!({ "element": "face", "solid": 3, "indices": [0] }));
        err(&mut s, "select_elements", json!({ "element": "vertex", "solid": 3, "indices": [] }));
    }

    #[test]
    fn carve_and_extrude_a_face_outline() {
        let mut s = hp_app::testing::test_state();
        let window = json!([[-64, 256, 32], [64, 256, 32], [64, 256, 96], [-64, 256, 96]]);
        let r = ok(&mut s, "extrude_face", json!({ "solid": 3, "side": 5, "points": window, "through": true }));
        assert_eq!(r["brushes"], 4);
        assert_eq!(r["mode"], "carve");
        assert_eq!(r["solid_ids"][0], 3);
        assert_eq!(s.doc.undo_label(), Some("MCP: extrude_face"));
        let vol: f64 = r["solid_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|id| s.doc.scene().solid(id.as_u64().unwrap() as u32).unwrap().geom.volume())
            .sum();
        assert!((vol - (512.0 * 16.0 * 128.0 - 128.0 * 16.0 * 64.0)).abs() < 1e-6);
        assert!(s.doc.undo());
        let l = json!([[-128, -128, 0], [128, -128, 0], [128, 0, 0], [0, 0, 0], [0, 128, 0], [-128, 128, 0]]);
        let r = ok(
            &mut s,
            "extrude_face",
            json!({ "solid": 2, "side": 0, "points": l, "mode": "extrude", "depth": 32, "material": "dev/dev_blendmeasure" }),
        );
        let ids = r["solid_ids"].as_array().unwrap();
        assert!(ids.len() >= 2, "a concave outline makes several convex brushes");
        for id in ids {
            let n = s.doc.scene().solid(id.as_u64().unwrap() as u32).unwrap();
            assert!(n.geom.valid);
            assert!(n.solid.sides.iter().all(|side| side.material == "dev/dev_blendmeasure"));
        }
        assert_eq!(r["bounds"]["max"][2], 32.0);
        let r = ok(
            &mut s,
            "extrude_face",
            json!({ "solid": 2, "side": 1, "points": [[-256, -256, -16], [256, -256, -16], [256, 256, -16], [-256, 256, -16]], "mode": "extrude", "depth": 8 }),
        );
        assert_eq!(r["solid_ids"], json!([2]), "an outline covering the face grows the brush");
        err(&mut s, "extrude_face", json!({ "solid": 3, "side": 99, "points": window }));
        err(&mut s, "extrude_face", json!({ "solid": 3, "side": 5, "points": [[0, 256, 0], [1, 256, 0]] }));
        err(
            &mut s,
            "extrude_face",
            json!({ "solid": 3, "side": 5, "points": [[0, 256, 0], [64, 256, 64], [64, 256, 0], [0, 256, 64]] }),
        );
        err(
            &mut s,
            "extrude_face",
            json!({ "solid": 3, "side": 5, "points": [[900, 256, 0], [964, 256, 0], [964, 256, 64]] }),
        );
        err(&mut s, "extrude_face", json!({ "solid": 3, "side": 5, "points": window, "depth": 0 }));
    }

    #[test]
    fn vertices_and_shapes() {
        let mut s = hp_app::testing::test_state();
        let v = ok(&mut s, "get_brush_vertices", json!({ "solid": 3 }));
        assert_eq!(v["vertices"].as_array().unwrap().len(), 8);
        assert_eq!(v["faces"].as_array().unwrap().len(), 6);
        let top: Vec<usize> = v["vertices"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, p)| p[2].as_f64() == Some(128.0))
            .map(|(i, _)| i)
            .collect();
        ok(&mut s, "move_brush_vertices", json!({ "solid": 3, "vertices": top, "offset": [0, 0, 64] }));
        let b = s.doc.scene().objects_bounds(&[ObjRef::Solid(3)], &|_| None).unwrap();
        assert_eq!(b.max.z, 192.0);
        err(&mut s, "move_brush_vertices", json!({ "solid": 3, "vertices": [99], "offset": [0, 0, 1] }));
        let cube = json!([[0, 0, 0], [64, 0, 0], [0, 64, 0], [64, 64, 0], [32, 32, 64]]);
        let r = ok(
            &mut s,
            "create_brush_from_points",
            json!({ "points": cube, "material": "dev/dev_measuregeneric01b", "entity_class": "func_detail" }),
        );
        assert!(r["entity_id"].as_u64().is_some());
        err(&mut s, "create_brush_from_points", json!({ "points": [[0, 0, 0], [1, 0, 0], [0, 1, 0], [1, 1, 0]] }));
        let r = ok(
            &mut s,
            "set_brush_vertices",
            json!({ "solid": 3, "points": [[0, 0, 0], [64, 0, 0], [0, 64, 0], [0, 0, 64]] }),
        );
        assert_eq!(r["faces"], 4);
        let r = ok(
            &mut s,
            "create_primitive",
            json!({ "shape": "arch", "min": [0, 0, 0], "max": [256, 256, 64], "sides": 6, "arch_arc": 180, "arch_wall_width": 16 }),
        );
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 6);
        err(&mut s, "create_primitive", json!({ "min": [0, 0, 0], "max": [0, 10, 10] }));
        let r = ok(
            &mut s,
            "create_primitive",
            json!({ "shape": "stairs", "min": [2000, 0, 0], "max": [2064, 128, 96], "stair_steps": 6 }),
        );
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 6);
        let r = ok(
            &mut s,
            "create_primitive",
            json!({ "shape": "doorway", "min": [2100, 0, 0], "max": [2228, 16, 128], "door_width": 48, "door_height": 96 }),
        );
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 3);
        for shape in ["spiral_stairs", "ramp", "pillar"] {
            let r =
                ok(&mut s, "create_primitive", json!({ "shape": shape, "min": [2300, 0, 0], "max": [2428, 128, 96] }));
            assert!(!r["solid_ids"].as_array().unwrap().is_empty(), "{shape}");
        }
        err(
            &mut s,
            "create_primitive",
            json!({ "shape": "ramp", "min": [0, 0, 0], "max": [64, 64, 64], "ramp_angle": 95 }),
        );
        err(
            &mut s,
            "create_primitive",
            json!({ "shape": "doorway", "min": [0, 0, 0], "max": [64, 16, 64], "door_width": 63 }),
        );
        s.doc.select(&[ObjRef::Solid(2)], SelectOp::Replace);
        let r = ok(
            &mut s,
            "create_primitive",
            json!({ "min": [600, 0, 500], "max": [664, 64, 900], "depth_from_selection": "z" }),
        );
        assert_eq!(r["bounds"]["min"][2], -16.0);
        assert_eq!(r["bounds"]["max"][2], 0.0);
        let r = ok(
            &mut s,
            "create_primitive",
            json!({
                "shape": "torus", "min": [700, 0, 0], "max": [956, 256, 256], "sides": 8, "torus_tube_sides": 4,
                "torus_arc": 720, "torus_rise": 128, "up": "x", "upside_down": true, "tex_scale": 0.5,
                "entity_class": "func_detail",
            }),
        );
        let entity = r["entity_id"].as_u64().unwrap() as u32;
        assert_eq!(s.doc.scene().entity(entity).unwrap().entity.classname, "func_detail");
        assert!(!r["solid_ids"].as_array().unwrap().is_empty());
        assert_eq!(s.doc.undo_label(), Some("MCP: create_primitive"));
        err(
            &mut s,
            "create_primitive",
            json!({ "shape": "torus", "min": [0, 0, 0], "max": [64, 64, 64], "torus_arc": 720 }),
        );
        err(&mut s, "create_primitive", json!({ "min": [0, 0, 0], "max": [64, 64, 64], "entity_class": "a b" }));
        let before =
            ok(&mut s, "find_material_usage", json!({ "pattern": "concrete/concretefloor001a" }))["faces_total"]
                .as_u64()
                .unwrap();
        let r = ok(
            &mut s,
            "create_room",
            json!({ "min": [1000, 0, 0], "max": [1256, 256, 128], "floor_material": "concrete/concretefloor001a" }),
        );
        assert_eq!(r["wall_ids"].as_array().unwrap().len(), 6);
        let u = ok(&mut s, "find_material_usage", json!({ "pattern": "concrete/concretefloor001a" }));
        assert_eq!(u["faces_total"].as_u64().unwrap(), before + 1, "the room floor");
        let r = ok(
            &mut s,
            "create_stairs",
            json!({ "min": [2000, 0, 0], "max": [2128, 64, 64], "steps": 8, "direction": "-x" }),
        );
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 8);
        assert_eq!(r["step_height"], 8.0);
        err(&mut s, "create_stairs", json!({ "min": [0, 0, 0], "max": [4, 64, 64], "steps": 8, "direction": "+x" }));
        let top_x = |s: &hp_app::AppState, ids: &serde_json::Value| {
            let id = ids.as_array().unwrap().iter().filter_map(serde_json::Value::as_u64).max_by_key(|i| {
                s.doc.scene().objects_bounds(&[ObjRef::Solid(*i as u32)], &|_| None).map_or(0, |b| b.max.z as i64)
            });
            let b = s.doc.scene().objects_bounds(&[ObjRef::Solid(id.unwrap() as u32)], &|_| None).unwrap();
            (b.min.x + b.max.x) * 0.5
        };
        let r = ok(
            &mut s,
            "create_primitive",
            json!({ "shape": "stairs", "min": [3000, 0, 0], "max": [3128, 64, 64], "direction": "-x" }),
        );
        assert!(top_x(&s, &r["solid_ids"]) < 3064.0);
        let r = ok(
            &mut s,
            "create_primitive",
            json!({ "shape": "stairs", "min": [3200, 0, 0], "max": [3328, 64, 64], "direction": "longest" }),
        );
        assert!(top_x(&s, &r["solid_ids"]) > 3264.0);
        err(
            &mut s,
            "create_primitive",
            json!({ "shape": "stairs", "min": [0, 0, 0], "max": [64, 64, 64], "direction": "up" }),
        );
        let r = ok(
            &mut s,
            "create_primitive",
            json!({
                "shape": "doorway", "min": [3400, 0, 0], "max": [3528, 16, 128], "door_width": 64,
                "door_height": 96, "door_arch": true, "door_arch_segments": 6,
            }),
        );
        assert_eq!(r["solid_ids"].as_array().unwrap().len(), 8);
        err(
            &mut s,
            "create_primitive",
            json!({ "shape": "doorway", "min": [0, 0, 0], "max": [128, 16, 128], "door_width": 64, "door_arch": true, "door_arch_radius": 10 }),
        );
        let r = ok(
            &mut s,
            "create_polygon_brush",
            json!({ "view": "top", "points": [[0, 0], [64, 0], [64, 64], [32, 96], [0, 64]], "depth": [0, 32] }),
        );
        let id = r["solid_id"].as_u64().unwrap() as u32;
        assert_eq!(s.doc.solid_vertices(id).unwrap().0.len(), 10);
        err(
            &mut s,
            "create_polygon_brush",
            json!({ "view": "top", "points": [[0, 0], [64, 64], [64, 0], [0, 64]], "depth": [0, 32] }),
        );
        err(
            &mut s,
            "create_polygon_brush",
            json!({ "view": "side", "points": [[0, 0], [64, 0], [0, 64]], "depth": [5, 5] }),
        );
        ok(&mut s, "move_brush_vertices", json!({ "solid": id, "vertices": [0], "offset": [0.3, 0.2, 0] }));
        let r = ok(&mut s, "snap_brush_vertices", json!({ "solid_ids": [id], "grid": 1 }));
        assert_eq!(r["solids_changed"], 1);
        assert!(s.doc.solid_vertices(id).unwrap().0.iter().all(|p| p.x.fract() == 0.0 && p.y.fract() == 0.0));
    }

    #[test]
    fn curve_brushes_follow_the_points() {
        let mut s = hp_app::testing::test_state();
        s.doc.clear_selection();
        let before = s.doc.stats().solids;
        let (undo, _) = s.doc.history_len();
        let r = ok(
            &mut s,
            "create_curve_brushes",
            json!({ "view": "top", "points": [[3, -2], [254, 130], [513, 0]], "subdivisions": 24, "thickness": 32, "height": 96, "base": 16, "material": "concrete/concretefloor001a" }),
        );
        assert_eq!(r["count"], 24);
        assert_eq!(s.doc.stats().solids, before + 24);
        assert_eq!(s.doc.history_len().0, undo + 1, "one undo step");
        assert_eq!(r["depth"], json!([16.0, 112.0]));
        let first = &r["points"][0];
        assert!(first[0].as_f64().unwrap() % 8.0 == 0.0 && first[1].as_f64().unwrap() % 8.0 == 0.0, "{first}");
        let ids: Vec<u32> = r["solid_ids"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
        assert_eq!(s.doc.selected_solid_ids().len(), 24, "selected");
        for id in &ids {
            let n = s.doc.scene().solid(*id).unwrap();
            assert!(n.geom.valid);
            assert_eq!(n.geom.faces.len(), 6);
            assert!(n.solid.sides.iter().all(|x| x.material == "concrete/concretefloor001a"));
            assert!((n.bounds.min.z - 16.0).abs() < 1e-6 && (n.bounds.max.z - 112.0).abs() < 1e-6);
        }
        let r = ok(
            &mut s,
            "create_curve_brushes",
            json!({ "view": "front", "points": [[0, 0], [128, 128]], "subdivisions": 2, "entity_class": "func_detail", "snap": false }),
        );
        assert_eq!(r["count"], 2);
        assert!(r["entity_id"].as_u64().is_some());
        let e = err(
            &mut s,
            "create_curve_brushes",
            json!({ "view": "side", "points": [[0, 0], [512, 0]], "subdivisions": 4096 }),
        );
        assert!(e.contains("under 1"), "{e}");
        let e = err(
            &mut s,
            "create_curve_brushes",
            json!({ "view": "top", "points": [[0, 0], [256, 0], [0, 8]], "thickness": 64 }),
        );
        assert!(e.contains("fold"), "{e}");
        let solids = s.doc.stats().solids;
        err(&mut s, "create_curve_brushes", json!({ "view": "top", "points": [[0, 0], [256, 0]], "subdivisions": 1 }));
        err(
            &mut s,
            "create_curve_brushes",
            json!({ "view": "top", "points": [[0, 0], [256, 0]], "subdivisions": 4097 }),
        );
        err(&mut s, "create_curve_brushes", json!({ "view": "top", "points": [[0, 0], [256, 0]], "thickness": 0.5 }));
        err(&mut s, "create_curve_brushes", json!({ "view": "top", "points": [[0, 0], [256, 0]], "thickness": 1025 }));
        err(&mut s, "create_curve_brushes", json!({ "view": "top", "points": [[0, 0]] }));
        err(&mut s, "create_curve_brushes", json!({ "view": "top", "points": [[0, 0], [1, 1]] }));
        err(&mut s, "create_curve_brushes", json!({ "view": "3d", "points": [[0, 0], [256, 0]] }));
        assert_eq!(s.doc.stats().solids, solids, "refused curves create nothing");
    }

    #[test]
    fn curves_are_read_and_edited() {
        let mut s = hp_app::testing::test_state();
        s.doc.clear_selection();
        let r = ok(
            &mut s,
            "create_curve_brushes",
            json!({ "view": "top", "points": [[0, 0], [128, 64], [256, 0]], "subdivisions": 6, "thickness": 32, "profile": "curb", "curb_width": 8, "curb_height": 4, "roll_start": 3, "auto_bank": 5 }),
        );
        let n = r["count"].as_u64().unwrap() as usize;
        assert!(n >= 18 && n.is_multiple_of(18), "3 brushes a piece, banked pieces split: {n}");
        assert_eq!(r["profile"], "curb");
        let first = r["solid_ids"][0].as_u64().unwrap();
        let before = s.doc.stats().solids;
        let (undo, _) = s.doc.history_len();
        let got = ok(&mut s, "get_curve_brushes", json!({ "solid_id": first }));
        assert_eq!(got["points"], json!([[0.0, 0.0], [128.0, 64.0], [256.0, 0.0]]));
        assert_eq!(got["solid_ids"].as_array().unwrap().len(), n);
        assert_eq!(got["roll_start"], 3.0);
        assert_eq!(got["auto_bank"], 5.0);
        let e = ok(
            &mut s,
            "edit_curve_brushes",
            json!({ "solid_id": first, "points": [[0, 0], [64, 32], [128, 64], [256, 0]], "profile": "ditch", "subdivisions": 5, "roll_end": -4 }),
        );
        let m = e["count"].as_u64().unwrap() as usize;
        assert!(m >= 10 && m.is_multiple_of(10), "2 brushes a piece: {m}");
        assert_eq!(s.doc.stats().solids, before - n + m);
        assert_eq!(s.doc.history_len().0, undo + 1, "one undo step");
        let again = ok(&mut s, "get_curve_brushes", json!({}));
        assert_eq!(again["profile"], "ditch");
        assert_eq!(again["points"].as_array().unwrap().len(), 4);
        assert_eq!(again["roll_start"], 3.0, "kept");
        assert_eq!(again["roll_end"], -4.0);
        let e = err(&mut s, "edit_curve_brushes", json!({ "profile": "ditch", "ditch_depth": 500 }));
        assert!(e.contains("ditch"), "{e}");
        assert_eq!(s.doc.stats().solids, before - n + m, "refused edits change nothing");
        let e = err(&mut s, "get_curve_brushes", json!({ "solid_id": 4_000_000 }));
        assert!(e.contains("not a brush made by the curve tool"), "{e}");
    }

    #[test]
    fn prop_to_brushes_needs_a_prop_and_its_model() {
        let mut s = hp_app::testing::test_state();
        let e = err(&mut s, "prop_to_brushes", json!({}));
        assert!(e.contains("select a prop"), "{e}");
        let kv = [("model".to_owned(), "models/a.mdl".to_owned())];
        let prop = s.doc.create_point_entity("prop_static", glam::DVec3::ZERO, &kv).unwrap();
        let e = err(&mut s, "prop_to_brushes", json!({ "id": prop, "shape": "cube" }));
        assert!(e.contains("unknown shape"), "{e}");
        let e = err(&mut s, "prop_to_brushes", json!({ "id": prop, "target": "sky" }));
        assert!(e.contains("unknown target"), "{e}");
        err(&mut s, "prop_to_brushes", json!({ "shape": "render", "keep_prop": true }));
        err(&mut s, "prop_to_brushes", json!({ "id": 2 }));
        assert!(s.doc.scene().entity(prop).is_some(), "nothing changed");
    }

    const TRAIN_FGD: &str = "@BaseClass = Origin [ origin(origin) : \"Origin\" ]\n\
        @SolidClass base(Origin) = func_tracktrain : \"Train\" [ target(target_destination) : \"First stop\" ]\n\
        @SolidClass base(Origin) = func_tanktrain : \"Tank\" []\n\
        @SolidClass = func_wall : \"Wall\" []";

    #[test]
    fn trains_get_an_origin_at_their_center() {
        let mut s = hp_app::testing::test_state_with_fgd(TRAIN_FGD);
        let center = s.doc.scene().solid(3).expect("solid 3").bounds.center();
        let r = ok(&mut s, "tie_to_entity", json!({ "solid_ids": [3], "entity_class": "func_tracktrain" }));
        let id = r["entity_id"].as_u64().unwrap() as u32;
        assert_eq!(s.doc.scene().entity(id).unwrap().entity.origin(), Some(center));
        assert_eq!(s.doc.undo_label(), Some("MCP: tie_to_entity"), "one undo step");
        let r = ok(&mut s, "tie_to_entity", json!({ "solid_ids": [2], "entity_class": "func_wall" }));
        let wall = r["entity_id"].as_u64().unwrap() as u32;
        assert!(s.doc.scene().entity(wall).unwrap().entity.get("origin").is_none(), "no origin key in the class");
        ok(&mut s, "set_entity_class", json!({ "ids": [wall], "classname": "func_tanktrain" }));
        assert!(s.doc.scene().entity(wall).unwrap().entity.origin().is_some());
        let r = ok(
            &mut s,
            "tie_to_entity",
            json!({ "solid_ids": [2], "entity_class": "func_tanktrain", "keyvalues": { "origin": "1 2 3" } }),
        );
        let given = r["entity_id"].as_u64().unwrap() as u32;
        assert_eq!(s.doc.scene().entity(given).unwrap().entity.get("origin"), Some("1 2 3"), "a given origin wins");
        ok(&mut s, "move_to_world", json!({ "ids": [given] }));
        let r = ok(&mut s, "tie_to_entity", json!({ "solid_ids": [2], "entity_class": "func_wall" }));
        let other = r["entity_id"].as_u64().unwrap() as u32;
        ok(&mut s, "set_entity_keyvalues", json!({ "id": other, "set": { "classname": "func_tracktrain" } }));
        assert!(s.doc.scene().entity(other).unwrap().entity.origin().is_some());
    }

    #[test]
    fn ties_and_faces() {
        let mut s = hp_app::testing::test_state();
        let r = ok(
            &mut s,
            "tie_to_entity",
            json!({ "solid_ids": [3], "entity_class": "func_wall", "keyvalues": { "targetname": "w" } }),
        );
        let ent = r["entity_id"].as_u64().unwrap();
        ok(&mut s, "tie_to_entity", json!({ "solid_ids": [2], "entity": ent }));
        assert_eq!(s.doc.scene().entity_solid_ids(ent as u32).len(), 2);
        err(&mut s, "tie_to_entity", json!({ "solid_ids": [2], "entity": 5 }));
        let r = ok(&mut s, "move_to_world", json!({ "ids": [ent] }));
        assert_eq!(r["solids_moved"], 2);
        let r = ok(&mut s, "tie_to_entity", json!({ "solid_ids": [2, 3], "each": true }));
        assert_eq!(r["entity_ids"].as_array().unwrap().len(), 2);

        let mut s = hp_app::testing::test_state();
        let f = ok(&mut s, "get_faces", json!({ "solid_ids": [2] }));
        assert_eq!(f["solids"][0]["faces"].as_array().unwrap().len(), 6);
        assert_eq!(f["solids"][0]["faces"][0]["vertices"].as_array().unwrap().len(), 4);
        ok(
            &mut s,
            "set_face_texture",
            json!({ "faces": [{ "solid": 2, "side": 0 }], "material": "concrete/concretefloor001a", "u_scale": 0.5, "rotate_by": 90, "lightmap_scale": 8 }),
        );
        let side = s.doc.face_side(hp_editor::FaceRef::new(2, 0)).unwrap();
        assert_eq!(side.material.to_ascii_lowercase(), "concrete/concretefloor001a");
        assert_eq!((side.uaxis.scale, side.lightmap_scale), (0.5, 8));
        err(&mut s, "set_face_texture", json!({ "faces": [{ "solid": 2, "side": 0 }], "u_scale": 0 }));
        ok(&mut s, "justify_texture", json!({ "solid_ids": [2], "mode": "fit", "treat_as_one": true }));
        ok(&mut s, "fit_texture", json!({ "faces": [{ "solid": 3, "side": 1 }], "repeat_u": 2 }));
        ok(&mut s, "align_texture_to_view", json!({ "solid_ids": [3], "view": "front" }));
        err(&mut s, "align_texture_to_view", json!({ "solid_ids": [3], "right": [1, 0, 0], "up": [2, 0, 0] }));
        ok(
            &mut s,
            "wrap_texture",
            json!({ "source": [{ "solid": 3, "side": 0 }], "faces": [{ "solid": 3, "side": 2 }] }),
        );
        let r = ok(&mut s, "replace_materials", json!({ "find": "concrete/*", "replace": "dev/dev_blendmeasure" }));
        assert!(r["faces"].as_u64().unwrap() >= 6);
        let r = ok(&mut s, "list_used_materials", json!({}));
        assert!(r["materials"].as_array().unwrap().iter().any(|m| m["material"] == "dev/dev_blendmeasure"));
        let r = ok(&mut s, "apply_nodraw", json!({ "faces": [{ "solid": 2, "side": 1 }] }));
        assert_eq!(r["faces"], 1);
        let r = ok(&mut s, "make_hint_brushes", json!({ "faces": [{ "solid": 3, "side": 0 }] }));
        assert_eq!(r["solids"], 1);
        err(&mut s, "replace_materials", json!({ "find": "a", "replace": "b", "solid_ids": [2] }));
    }
}
