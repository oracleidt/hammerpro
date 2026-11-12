use glam::{DVec2, DVec3};
use hp_app::settings::{Disp2dSetting, Disp2dViewSetting, RenderModeSetting};
use hp_app::view::Camera2d;
use hp_app::{Action, AppState, QuadrantView, ViewKind};
use hp_editor::limits::world_light::LightMode;
use hp_mcp::protocol::ToolResult;
use hp_mcp::tools::{ToolKind, ToolSpec};
use serde_json::{Value, json};

use super::automation::{UiOp, quadrant_view_name, view_name};
use super::schema::{boolean, empty, integer, number, object, one_of, string, vec3};
use super::tools::{Args, Domain, Outcome, R, args, v3};

pub(crate) const DOMAIN: Domain = Domain { category: "views", specs: SPECS, run };

const KINDS: &[&str] = &["3d", "top", "front", "side", "logical"];
const LOGICAL_FILTERS: &[&str] = &["all", "named", "connected"];
const VIEWS_2D: &[&str] = &["top", "front", "side", "all"];
const MAXIMIZE: &[&str] = &["0", "1", "2", "3", "none"];
const DISP_2D: &[&str] = &["global", "mesh", "selected_mesh", "outline"];
const MODES: &[(&str, Action, RenderModeSetting)] = &[
    ("textured", Action::RenderTextured, RenderModeSetting::Textured),
    ("flat", Action::RenderFlat, RenderModeSetting::Flat),
    ("wireframe", Action::RenderWireframe, RenderModeSetting::Wireframe),
    ("textured_wire", Action::RenderTexturedWire, RenderModeSetting::TexturedWire),
    ("lit", Action::RenderLit, RenderModeSetting::Lit),
    ("unshaded", Action::RenderUnshaded, RenderModeSetting::Unshaded),
    ("smoothing_groups", Action::RenderSmoothingGroups, RenderModeSetting::SmoothingGroups),
    ("baked", Action::RenderBaked, RenderModeSetting::Baked),
    ("lightmap_grid", Action::RenderLightmapGrid, RenderModeSetting::LightmapGrid),
    ("color_corrected", Action::RenderColorCorrected, RenderModeSetting::ColorCorrected),
];
const MODE_NAMES: &[&str] = &[
    "textured",
    "flat",
    "wireframe",
    "textured_wire",
    "lit",
    "unshaded",
    "smoothing_groups",
    "baked",
    "lightmap_grid",
    "color_corrected",
];

const SPECS: &[ToolSpec] = &[
    ToolSpec {
        name: "get_views",
        title: "The views and cameras",
        description: "The four views (kind: 3d, top, front, side or logical, the 2D logical view showing the \
            entities and their outputs as a schematic; rectangle in window points, maximized, focused), the 3D \
            camera, the 2D cameras (center, zoom), the 3D render mode, how the 2D views draw displacements (mesh \
            or outline, per view) and whether walkability colours are on, the grid (mode and the grid of each \
            view; list_grids has the details) and the saved layouts.",
        kind: ToolKind::Read,
        schema: empty,
    },
    ToolSpec {
        name: "get_logical_graph",
        title: "The 2D logical view graph",
        description: "What the 2D logical view (and the logical view window) shows: one node per entity (id, \
            class, name, the icon material from the FGD iconsprite or sprite helper, hidden when its visgroup is \
            hidden, outputs, inputs, problems) and one edge per output and target (from, to or null when no entity \
            has the name, output, target, input, parameter, delay, times, state: valid, missing_target, \
            unknown_output or unknown_input). filter is the filter of the view: all (default, every entity, \
            lights and info entities included, like Hammer), named (named or connected) or connected (with an \
            outgoing or incoming output).",
        kind: ToolKind::Read,
        schema: || {
            object(
                json!({
                    "filter": one_of("Default all.", LOGICAL_FILTERS),
                    "limit": integer("Most nodes listed (default 2000).", 1, 100_000),
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "set_view_layout",
        title: "Arrange the views",
        description: "Choose what each quadrant shows (quadrants: 4 of 3d, top, front, side, logical; top-left, \
            top-right, bottom-left, bottom-right; logical is Hammer's \"2D Logical\" view: the entities and their \
            outputs as a schematic, its selection shared with the other views; drive it with ui_click, ui_drag \
            and ui_scroll with view \"logical\" and look at it with screenshot_window), the split positions, \
            maximize one view (or none), how each 2D view draws displacements (displacements_2d: 4 of global, \
            mesh, selected_mesh, outline; like the view menu) and whether 2D views draw the brushes displacements were built from as a faint outline \
            (disp_source_brushes), or apply a saved layout by name (\"default\": the layout of a new installation).",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "quadrants": { "type": "array", "items": { "type": "string", "enum": KINDS }, "minItems": 4, "maxItems": 4, "description": "What each quadrant shows: a view kind, or logical for the 2D logical view." },
                    "displacements_2d": { "type": "array", "items": { "type": "string", "enum": DISP_2D }, "minItems": 4, "maxItems": 4, "description": "Per quadrant, how a 2D view draws displacements: global (the render.displacements_2d setting), mesh (the triangle grid), selected_mesh (grid of the selected ones, outline of the others) or outline." },
                    "disp_source_brushes": { "type": "boolean", "description": "2D views draw the brush each displacement was built from as a faint outline (render.disp_source_2d)." },
                    "split_x": number("Vertical split, fraction of the width.", 0.1, 0.9),
                    "split_y": number("Horizontal split, fraction of the height.", 0.1, 0.9),
                    "maximize": one_of("Quadrant to show alone, or none.", MAXIMIZE),
                    "layout": string("A saved layout name (get_views lists them) or \"default\"."),
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "set_render_mode",
        title: "3D render mode",
        description: "How the 3D view draws the map: textured (with the editor's shading), flat, wireframe, \
            textured_wire, lit (lighting preview: the light entities with VRAD's falloff and spot cones, shaded like the game's \
            LDR lightmaps when the light values are LDR, water and translucent brushes casting no shadow, \
            light_dynamic (an engine dynamic light: it also lights the baked preview and the player view over the \
            lightmaps, like in game), point_spotlight (with Start On: the engine beam, one camera facing sprites/glow_test02 strip \
            SpotlightWidth on each side of its axis, along SpotlightLength and stopped by the world, fading toward \
            its end and when seen along its axis, and the sprites/light_glow03 halo seen from the front of the \
            spotlight, in rendercolor (the beam at brightness 64) times HDRColorScale in HDR, in every 3D mode and \
            the player view; unless No Dynamic \
            Light, the dynamic light the game puts where the beam ends, in lit, baked and the player view; the player \
            view plays its LightOn and LightOff inputs; toolsskybox faces show the sky like the baked preview), the sun of the first light_environment (angles or pitch key, target, SunSpreadAngle soft \
            shadows) lighting only what sees a sky face toward it like VRAD, each point shadowed only by what lies \
            between it and the sky it reaches, skies stacked one above the other included (the 3D skybox, what lies beyond the sky \
            and brush entities without vrad_brush_cast_shadows cast no shadow, editor helpers neither), its _ambient \
            sky light only where the sky is seen (setting render.sky_occlusion), HDR or LDR light values (setting \
            render.light_preview_hdr), every light of big maps kept (get_lighting_status realtime tells how many and \
            how many crowded spots keep only their strongest lights), env_projectedtexture entities (setting \n            render.projected_textures: texture, lightfov, nearz, farz, lightcolor, lightstrength and target, the first 16; \n            get_lighting_status realtime projectors counts them), props included (prop_static shadows follow \
            the compile like VRAD: the model triangles with -StaticPropPolys, the collision model otherwise, none \
            with -nostaticpropshadows; setting render.prop_shadows: Auto, Mesh or Collision); the point and spot lights in view cast shadows too (the brightest first, so a light never shines through walls), how many set by the setting render.light_shadows (0 to 64, 16 by default, 0: the sun only; disableshadows props cast none); the sky light and the light bounced by the other surfaces come from a background bake of the map with the compiler while the setting render.lit_indirect is on (the default; get_lighting_status realtime.indirect tells its state), like the game's lightmaps, the lights and the sun staying real time; lights inside an opaque \
            brush light nothing like in game; $selfillum glows by the base alpha, $selfillummask or the $envmapmask alpha of $selfillum_envmapmask_alpha, times $selfillumtint, also in baked and the player view), unshaded (textured without shading), smoothing_groups, baked \
            (the baked lighting preview, see bake_lighting; props lit by the bake like the game: ambient cubes, \
            world lights, static prop vertex lighting), lightmap_grid (Hammer's 3D lightmap grid, no bake needed: every lightmapped face shows its luxel grid, one checker cell per lightmapscale units along its texture axes, displacements included, grey at scale 16, redder when finer, greener when coarser, yellow lines where VBSP splits a face wider than 32 luxels, shaded by the baked lighting once a bake exists; faces without a lightmap (nodraw, tool textures, sky, water, unlit) and props keep their texture), or color_corrected (3D Color Correction, Alt+9: the textured view with the color_correction_* entities' LUTs applied by camera distance, the zone weights of color_correction_zones, without turning on lab.color_correction_preview; a color_correction_preview call is shown too). In the textured modes water surfaces are drawn like in game (refraction, water fog, reflection, moving waves) while the setting render.water_preview is on (set_setting, or run_action toggle_water_preview); off, they show their tool texture. Expensive water reflects the scene drawn again mirrored about its surface like the game; render.water_detail follows the Water Detail option of the game: ReflectAll (default, expensive at every distance, reflects the world, props, brush entities, the sky and the 3D skybox), ReflectWorld (expensive near the camera, props and brush entities only with $reflectentities) or Simple (cheap water). \
            The map fog of an env_fog_controller (fogenable, fogcolor, fogcolor2 with fogblend, fogstart, fogend, \
            fogmaxdensity, farz) covers the 3D views and the player view while render.fog_preview is on (the \
            default; run_action toggle_fog_preview, preview_fog chooses the controller). Every fog_volume is \
            outlined and filled with its fog, the box around its brushes like the game, as thick as the view \
            crosses it, with a label naming its controller and its problems, while render.fog_volume_preview is on \
            (run_action toggle_fog_volumes): the fog of the env_fog_controller its FogName names (fogcolor, \
            fogcolor2 with fogblend along fogdir or the angles with use_angles, fogstart, fogend, fogmaxdensity), \
            or a light blue default look when it names nothing or a controller with fogenable 0 (list_fog tells \
            which). In the player view the controller fog replaces the map fog while the eyes are inside an \
            enabled volume (none with fogenable 0). render.fog_volume_texture (run_action \
            toggle_fog_volume_texture) draws or hides the brush material of the volumes in the 3D view. Every \
            func_fish_pool shows fish_count copies of its model (models/Junkola.mdl by default; a model the game \
            content lacks is previewed with models/props/de_inferno/goldfish.mdl) scattered within \
            max_range (at most 255) of the entity, the same places each time, kept in the water around the entity \
            8 units under its surface, while render.fish_preview is on (run_action toggle_fish_preview); they swim \
            slow circles while render.fish_swim is on (run_action toggle_fish_swim); selected, the pool shows a sphere \
            and a circle of its max_range. \
            Lightmapped_4WayBlend displacements show their 4 textures in every textured mode (textured, lit, \
            baked, the player view) like the game: texture 2, 3 then 4 laid over texture 1 by the multiblend \
            rows of the displacement (or its vertex alpha for texture 2 when it has none), shaped by \
            $textureN_lumstart and lumend, $textureN_blendstart and blendend, $lumblendfactorN, \
            $textureN_uvscale and $textureN_blendmode. \
            The normal maps of the materials ($bumpmap, $bumpmap2 of WorldVertexTransition and of \
            Lightmapped_4WayBlend, mixed by the same blend weights as the base textures, its \
            $textureN_bumpblendfactor choosing the map of each texture; $ssbump) give the surfaces their relief \
            in textured (the editor shading), lit (the preview lights), baked (the bumped lightmaps of the \
            preview bake, like the game), the player view (the compiled bumped lightmaps like the game), on the \
            props and in the model browser (preview and thumbnails) while the setting render.normal_maps is on \
            (the default; set_setting); off, surfaces are flat and no normal map is loaded. \
            Every prop is drawn at the level of detail the game picks for it, from the switch distances of \n            its model and the size it takes on the screen (setting render.model_lods, on by default; off, \n            the finest level is always drawn). The env_sun entity draws its sun sprite and its glow like the \n            game (direction, size, overlaysize, rendercolor, overlaycolor, material, overlaymaterial, \n            HDRColorScale, the fade over the 40 degrees around the middle of the view and the fade behind \n            what hides the sun) while the setting render.sun_glow is on (the default). The 3D views sample \n            their pixels on the game's pixel grid (setting render.game_pixel_grid, on by default: half a \n            pixel from the grid Vulkan and Direct3D 12 use, so a screenshot of the preview lines up with a \n            screenshot of the game). \n            The materials with $envmap reflect their cubemap (like the SDK 2013 shaders: along the bumped normal, \
            times $envmapmask or the base or normal map alpha, $envmaptint, $envmapcontrast, $envmapsaturation, \
            $fresnelreflection, $envmapfresnel with $phongfresnelranges on phong props, $envmaplightscale) in \
            textured, lit and baked and in the player view while the setting render.cubemaps is on (off by default \
            so the lighting preview shows the lights alone, settings of older versions are switched off once; \
            run_action toggle_cubemaps or set_setting turns it on): the editor draws a cubemap at every env_cubemap (cubemapsize, the sky \
            without any), each brush face reflects the nearest env_cubemap in front of it like the compiler (or \
            the one whose sides key lists it), props the nearest one; a face whose cubemap is missing or not drawn yet reflects the sky darkened by its \
            own lighting (like $envmaplightscale 1) instead of a bright sky. Parallax corrected cubemaps (Mapbase): \
            an env_cubemap whose parallaxobb key names a parallax_obb brush entity (in the player view, the \
            keys of the materials the compiler patched) makes the LightmappedGeneric and WorldVertexTransition \
            faces reflecting it look the cubemap up corrected to that box. The cubemaps are drawn like the first 3D \
            view (its mode, the player view's compiled lighting), a few faces a frame, again a moment after the \
            map changes while render.cubemaps_auto is on, or with run_action rebuild_cubemaps. In the player view \
            the cubemaps built in game are used when the compiled map has them (see player_view); \
            run_action write_cubemaps (from the player view) writes the drawn ones into the compiled map like \
            buildcubemaps. Asking for the mode the view already has changes nothing (changed false).",
        kind: ToolKind::Editor,
        schema: || object(json!({ "mode": one_of("Render mode.", MODE_NAMES) }), &["mode"]),
    },
    ToolSpec {
        name: "player_view",
        title: "The player view and its mat_wireframe",
        description: "Start or leave the player view (walk the map like in game, F6) and choose its wireframe, \
            Garry's Mod's mat_wireframe (X in the view): 0 off; 1 the triangles of the world surfaces in pink \
            seen through everything; 2 the same in green on the visible surfaces only; 3 the outlines of the \
            world polygons in pink seen through everything. In 1 to 3 the props are drawn only as the triangles \
            of the LOD the game picks (cyan, seen through), detail sprites as green lines, displacements are not \
            drawn, the world keeps its textures, like in game. Without arguments it only reports. The answer \
            gives the state and, with a compiled map, what the game draws through the area portals from the \
            camera: the area of the eye, the areas drawn, the area portals open and looked through (closed \
            doors hide what is behind them, an open one shows only what is seen through its opening). \
            compiled_lighting turns on or off (L in the view, setting render.player_compiled_lighting) the \
            lighting of the game: the compiled world drawn with the lightmaps of the compiled map (HDR when it \
            has them, else LDR, fullbright when it was compiled without VRAD) and the props lit by their baked \
            vertex lighting (static props) or the ambient cube and world lights at their lighting origin; the \
            answer tells whether the compiled lightmaps are used and which range (lighting), and where the \
            reflections come from (cubemaps: the samples of the map, how many reflect the cubemap built in game \
            and how many the one the editor draws there because the map holds VBSP's placeholders only). Light styles play like the engine (ten pattern letters a second: the built-in styles 1 to 12 such as \
            1 flicker or 10 fluorescent flicker, the patterns of named lights, style 63 dark): the lightmaps with \
            styles, the styled world lights on the models and the detail props follow them; light_styles reports \
            the time, every style the lights use with its string and value now, and the named (switchable) \
            lights with their state. light (a named light) with on (true or false; missing: toggle) switches it \
            in the preview at the next frame of the player view, like the inputs TurnOn, TurnOff and Toggle \
            (the keys 1 to 9 in the view do the same, the I/O simulation too); the map does not change. The point_spotlight beams and halos are drawn, and their dynamic light (unless No Dynamic Light) and light_dynamic entities light the compiled world and the props over the lightmaps like the engine's dynamic lights; the I/O simulation plays the point_spotlight inputs LightOn and LightOff. A brush with a displacement keeps only its displacement surface, like VBSP and the game: its other faces are neither drawn nor solid (compiled world and editor brushes alike). The I/O simulation also plays the env_projectedtexture inputs TurnOn, TurnOff, FOV, EnableShadows, SpotlightTexture and LightWorld on the projected light of the preview, and a named infodecal appears when an Activate input reaches it. \
            Trains move like in game: func_tracktrain and func_tanktrain follow their path_track chain from \
            their target (speed is the initial speed, startspeed the max speed, height above the track, wheels \
            for turning, orientationtype, velocitytype, bank, the No Pitch, No User Control, Passable and Fixed \
            Orientation spawnflags, path_track speed for trains without user control, disabled tracks, altpath \
            branches, loops, dead ends), func_train goes from path_corner to path_corner waiting their wait; \
            the player rides them, their parented entities move with them, their I/O inputs (StartForward, \
            SetSpeed, Toggle, EnablePath...) and outputs (OnPass, OnStart) run in the I/O simulation. train (a \
            train targetname, or * for every train) with start (true or false; missing: toggle) starts or stops \
            trains in the running player view (T in the view does it for all, E on a train for one); trains \
            reports each train (moving, speed, position, angles, the path node it passed and the next one).",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "active": boolean("Start (true) or leave (false) the player view."),
                    "wireframe": integer("mat_wireframe: 0 off, 1, 2 or 3.", 0, 3),
                    "compiled_lighting": boolean("Light the player view like the game with the compiled lightmaps (true, the default) or keep the editor shading (false)."),
                    "light": string("A named (switchable) light of the compiled map to switch in the preview (its targetname)."),
                    "on": boolean("With light: turn it on (true) or off (false); missing: toggle."),
                    "train": string("A train of the map to start or stop in the player view (its targetname), or * for every train."),
                    "start": boolean("With train: start it (true) or stop it (false); missing: toggle."),
                    "eye": vec3("Place the eyes of the player at this point [x, y, z] (standing: the feet go 64 units lower)."),
                    "feet": vec3("Place the feet of the player at this point [x, y, z]."),
                    "yaw": number("Look direction in degrees (0 faces +X, 90 faces +Y).", -360.0, 360.0),
                    "pitch": number("Look up (positive) or down (negative) in degrees.", -89.0, 89.0),
                    "noclip": boolean("Fly through walls (true) or walk and fall (false); placing the eyes at a given point needs noclip so the player does not fall."),
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "zoom_2d",
        title: "Zoom a 2D view",
        description: "Zoom the top, front or side view (or all): an absolute zoom in points per unit, or a factor \
            (2 zooms in twice), optionally keeping a world point in place.",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "view": one_of("Default all.", VIEWS_2D),
                    "zoom": number("Screen points per world unit.", Camera2d::MIN_ZOOM, Camera2d::MAX_ZOOM),
                    "factor": number("Multiply the zoom by this.", 0.01, 100.0),
                    "anchor": vec3("World point that stays in place [x, y, z] (default the view center)."),
                }),
                &[],
            )
        },
    },
    ToolSpec {
        name: "pan_2d",
        title: "Pan a 2D view",
        description: "Move the top, front or side view (or all): center it on a world point, or move it by [right, \
            up] world units of that view.",
        kind: ToolKind::Editor,
        schema: || {
            object(
                json!({
                    "view": one_of("Default all.", VIEWS_2D),
                    "center": vec3("World point to center on [x, y, z]."),
                    "by": { "type": "array", "items": { "type": "number" }, "minItems": 2, "maxItems": 2, "description": "[right, up] in world units of the view." },
                }),
                &[],
            )
        },
    },
];

fn run(state: &mut AppState, name: &str, v: &Value) -> Option<Outcome> {
    let result = match name {
        "get_views" => super::tools::no_args(v).map(|()| Outcome::Ui(UiOp::Views)),
        "get_logical_graph" => logical_graph(state, v).map(Outcome::Done),
        "set_view_layout" => set_view_layout(state, v).map(Outcome::Done),
        "set_render_mode" => set_render_mode(state, v),
        "zoom_2d" => zoom_2d(state, v).map(Outcome::Done),
        "player_view" => player_view(state, v).map(Outcome::Done),
        "pan_2d" => pan_2d(state, v).map(Outcome::Done),
        _ => return None,
    };
    Some(result.unwrap_or_else(|e| Outcome::Done(ToolResult::error(format!("{name}: {e}")))))
}

fn view_of(name: &str) -> Option<QuadrantView> {
    Some(match name {
        "3d" => QuadrantView::View(ViewKind::Perspective),
        "top" => QuadrantView::View(ViewKind::Top),
        "front" => QuadrantView::View(ViewKind::Front),
        "side" => QuadrantView::View(ViewKind::Side),
        "logical" | "2d_logical" => QuadrantView::Logical,
        _ => return None,
    })
}

fn disp_2d_name(m: Disp2dSetting) -> &'static str {
    match m {
        Disp2dSetting::Mesh => "mesh",
        Disp2dSetting::SelectedMesh => "selected_mesh",
        Disp2dSetting::Outline => "outline",
    }
}

fn disp_2d_json(state: &AppState) -> Value {
    let r = &state.settings.render;
    let views: Vec<Value> = (0..4)
        .map(|i| {
            let own = match r.displacements_2d_views[i] {
                Disp2dViewSetting::Global => "global",
                Disp2dViewSetting::Own(m) => disp_2d_name(m),
            };
            json!({ "choice": own, "drawn": disp_2d_name(r.displacements_2d_of(i)) })
        })
        .collect();
    json!({
        "global": disp_2d_name(r.displacements_2d),
        "views": views,
        "walkability": r.disp_walkable,
        "source_brushes": r.disp_source_2d,
    })
}

fn cam2d_json(c: Camera2d) -> Value {
    json!({ "center": [c.center.x, c.center.y], "zoom": c.zoom })
}

pub(crate) fn views_json(
    state: &AppState,
    rects: [Option<(ViewKind, egui::Rect)>; 4],
    logical: [Option<egui::Rect>; 4],
) -> Value {
    let ui = &state.settings.ui;
    let views: Vec<Value> = (0..4)
        .map(|i| {
            let view = hp_ui_viewports::layout::quadrant_view(&ui.quadrants, i);
            let shown = match view {
                QuadrantView::Logical => logical[i],
                QuadrantView::View(_) => rects[i].map(|(_, r)| r),
            };
            let rect = shown.map(|r| json!([r.left().round(), r.top().round(), r.width().round(), r.height().round()]));
            json!({
                "index": i,
                "kind": quadrant_view_name(view),
                "visible": shown.is_some(),
                "rect": rect,
                "maximized": state.ui.maximized_view == Some(i),
                "focused": state.ui.focused_view == Some(i),
            })
        })
        .collect();
    let c = &state.cameras.cam3d;
    let mode = MODES.iter().find(|(_, _, m)| *m == state.settings.view.render_mode).map_or("textured", |(n, _, _)| n);
    let kinds =
        [("3d", ViewKind::Perspective), ("top", ViewKind::Top), ("front", ViewKind::Front), ("side", ViewKind::Side)];
    let grids: Vec<Value> = kinds
        .iter()
        .map(|(n, k)| {
            let f = state.grid_frame(*k);
            json!({ "view": n, "size": f.nominal(), "step": v3(f.step), "origin": v3(f.origin), "rotated": f.is_rotated() })
        })
        .collect();
    json!({
        "views": views,
        "split": [ui.split_x, ui.split_y],
        "camera_3d": { "position": v3(c.pos), "yaw": c.yaw_deg, "pitch": c.pitch_deg, "fov": state.settings.view.fov_deg },
        "cameras_2d": {
            "top": cam2d_json(state.cameras.top),
            "front": cam2d_json(state.cameras.front),
            "side": cam2d_json(state.cameras.side),
        },
        "render_mode": mode,
        "displacements_2d": disp_2d_json(state),
        "grid": {
            "size": state.grid_size(),
            "snap": state.settings.edit.snap_to_grid,
            "shown": state.settings.edit.show_grid,
            "mode": state.settings.grid.mode.name(),
            "per_view": grids,
        },
        "layouts": state.settings.workflow.layouts.keys().collect::<Vec<_>>(),
    })
}

fn set_view_layout(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a =
        args(v, &["quadrants", "split_x", "split_y", "maximize", "layout", "displacements_2d", "disp_source_brushes"])?;
    if a.map.is_empty() {
        return Err(
            "give quadrants, split_x, split_y, maximize, displacements_2d, disp_source_brushes or layout".to_owned()
        );
    }
    let source = a.opt_bool("disp_source_brushes")?;
    let disp_2d = match a.get("displacements_2d") {
        None => None,
        Some(d) => {
            let bad =
                || "displacements_2d must be 4 of \"global\", \"mesh\", \"selected_mesh\", \"outline\"".to_owned();
            let list = d.as_array().filter(|l| l.len() == 4).ok_or_else(bad)?;
            let mut out = [Disp2dViewSetting::Global; 4];
            for (o, x) in out.iter_mut().zip(list) {
                *o = match x.as_str().map(str::to_ascii_lowercase).as_deref() {
                    Some("global") => Disp2dViewSetting::Global,
                    Some("mesh") => Disp2dViewSetting::Own(Disp2dSetting::Mesh),
                    Some("selected_mesh") => Disp2dViewSetting::Own(Disp2dSetting::SelectedMesh),
                    Some("outline") => Disp2dViewSetting::Own(Disp2dSetting::Outline),
                    _ => return Err(bad()),
                };
            }
            Some(out)
        }
    };
    let layout = a.opt_str("layout", 128)?;
    if layout.is_some() && a.map.len() > 1 {
        return Err("layout applies a whole saved layout: give it alone".to_owned());
    }
    let quadrants = match a.get("quadrants") {
        None => None,
        Some(q) => {
            let bad = || "quadrants must be 4 of \"3d\", \"top\", \"front\", \"side\", \"logical\"".to_owned();
            let list = q.as_array().filter(|l| l.len() == 4).ok_or_else(bad)?;
            let mut out = [0u8; 4];
            for (o, x) in out.iter_mut().zip(list) {
                let view = x.as_str().map(str::to_ascii_lowercase).and_then(|s| view_of(s.trim())).ok_or_else(bad)?;
                *o = view.index();
            }
            Some(out)
        }
    };
    let (smin, smax) = (f64::from(hp_ui_viewports::layout::SPLIT_MIN), f64::from(hp_ui_viewports::layout::SPLIT_MAX));
    let split_x = a.opt_f64("split_x", smin, smax)?;
    let split_y = a.opt_f64("split_y", smin, smax)?;
    let maximize = a.opt_enum("maximize", MAXIMIZE)?;
    if let Some(name) = layout {
        let saved = if name.eq_ignore_ascii_case("default") {
            hp_app::settings::SavedLayout::default()
        } else {
            let layouts = &state.settings.workflow.layouts;
            layouts.get(name).cloned().ok_or_else(|| {
                let names: Vec<&str> = layouts.keys().map(String::as_str).collect();
                format!("no saved layout \"{name}\" (saved: {}; or \"default\")", names.join(", "))
            })?
        };
        crate::dispatch::workflow::apply_layout(state, &saved);
    }
    let ui = &mut state.settings.ui;
    if let Some(q) = quadrants {
        ui.quadrants = q;
    }
    if let Some(x) = split_x {
        ui.split_x = x as f32;
    }
    if let Some(y) = split_y {
        ui.split_y = y as f32;
    }
    if let Some(m) = maximize {
        state.ui.maximized_view = m.parse::<usize>().ok();
    }
    if let Some(d) = disp_2d {
        state.settings.render.displacements_2d_views = d;
    }
    if let Some(on) = source {
        state.settings.render.disp_source_2d = on;
    }
    state.settings_dirty = true;
    let ui = &state.settings.ui;
    let kinds: Vec<&str> =
        (0..4).map(|i| quadrant_view_name(hp_ui_viewports::layout::quadrant_view(&ui.quadrants, i))).collect();
    Ok(ToolResult::json(json!({
        "quadrants": kinds,
        "split": [ui.split_x, ui.split_y],
        "maximized": state.ui.maximized_view,
        "displacements_2d": disp_2d_json(state),
    })))
}

fn set_render_mode(state: &AppState, v: &Value) -> R<Outcome> {
    let a = args(v, &["mode"])?;
    let mode = a.opt_enum("mode", MODE_NAMES)?.ok_or("mode is required")?;
    let (_, action, setting) = MODES.iter().find(|(n, _, _)| *n == mode).ok_or("mode is required")?;
    if state.settings.view.render_mode == *setting {
        return Ok(Outcome::Done(ToolResult::json(json!({ "mode": mode, "changed": false }))));
    }
    Ok(Outcome::Ui(UiOp::RunAction(*action)))
}

const WIREFRAME_LEVELS: [&str; 4] = [
    "off",
    "world triangles seen through, models in wireframe",
    "world triangles on the visible surfaces, models in wireframe",
    "world polygon outlines seen through, models in wireframe",
];

fn player_view(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(
        v,
        &[
            "active",
            "wireframe",
            "compiled_lighting",
            "light",
            "on",
            "train",
            "start",
            "eye",
            "feet",
            "yaw",
            "pitch",
            "noclip",
        ],
    )?;
    let on = a.opt_bool("on")?;
    match a.opt_str("light", 256)?.map(str::trim) {
        Some(name) => {
            let table = state.runtime.health.world.as_ref().map(|w| &w.light.styles);
            let Some(i) = table.and_then(|t| t.find(name)) else {
                let names: Vec<&str> =
                    table.map(|t| t.lights.iter().map(|l| l.name.as_str()).collect()).unwrap_or_default();
                return Err(if names.is_empty() {
                    "the compiled map has no named (switchable) light".to_owned()
                } else {
                    format!("no named light {name:?}; the named lights are: {}", names.join(", "))
                });
            };
            let name = table.map(|t| t.lights[i].name.clone()).unwrap_or_default();
            state.runtime.lab.player.light_requests.push((name, on));
        }
        None if on.is_some() => return Err("on needs light".into()),
        None => {}
    }
    let start = a.opt_bool("start")?;
    match a.opt_str("train", 256)?.map(str::trim) {
        Some(name) => {
            let names = map_trains(state);
            let all = name == "*" || name.is_empty();
            if names.is_empty() {
                return Err("the map has no func_tracktrain, func_tanktrain or func_train".to_owned());
            }
            if !all && !names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                return Err(format!("no train named {name:?}; the named trains are: {}", names.join(", ")));
            }
            let p = &state.runtime.lab.player;
            if !p.active && !p.start_request && a.opt_bool("active")? != Some(true) {
                return Err("the trains move in the player view only: start it with active true".to_owned());
            }
            let name = if all { None } else { Some(name.to_owned()) };
            state.runtime.lab.player.train_requests.push((name, start));
        }
        None if start.is_some() => return Err("start needs train".into()),
        None => {}
    }
    let place = hp_app::runtime::PlayerPlace {
        feet: a.opt_vec3("feet")?,
        eye: a.opt_vec3("eye")?,
        yaw: a.opt_f64("yaw", -360.0, 360.0)?,
        pitch: a.opt_f64("pitch", -89.0, 89.0)?,
        noclip: a.opt_bool("noclip")?,
    };
    if place != hp_app::runtime::PlayerPlace::default() {
        let p = &state.runtime.lab.player;
        if !p.active && !p.start_request && a.opt_bool("active")? != Some(true) {
            return Err("placing the player needs the player view: start it with active true".to_owned());
        }
        state.runtime.lab.player.place_request = Some(place);
    }
    if let Some(on) = a.opt_bool("compiled_lighting")? {
        state.settings.render.player_compiled_lighting = on;
        state.settings_dirty = true;
    }
    let level = a.opt_f64("wireframe", 0.0, 3.0)?;
    if let Some(level) = level {
        if level.fract() != 0.0 {
            return Err("wireframe must be 0, 1, 2 or 3".into());
        }
        state.runtime.lab.player.wireframe = level as u8;
        if level > 0.0 {
            state.assets.set_model_lods(true);
        }
    }
    match a.opt_bool("active")? {
        Some(true) if !state.runtime.lab.player.active => state.runtime.lab.player.start_request = true,
        Some(false) => state.runtime.lab.player.active = false,
        _ => {}
    }
    let p = &state.runtime.lab.player;
    let rt = &state.runtime.health;
    let areas = rt.play_flow.as_deref().map(|f| {
        json!({
            "area": f.own_area,
            "areas_drawn": f.visible_areas(),
            "areas": f.visible.len(),
            "portals": f.portals,
            "portals_open": f.portals_open,
            "portals_looked_through": f.portals_seen,
        })
    });
    let level = usize::from(p.wireframe.min(3));
    let eye = p.feet + glam::DVec3::new(0.0, 0.0, hp_ui_viewports::player::EYE);
    let compiled_world = rt.world.is_some() && !p.editor_world;
    let setting = state.settings.render.player_compiled_lighting;
    let stats = state.gfx.as_ref().and_then(|g| g.renderer.compiled_lighting());
    let mode = rt.world.as_ref().map(|w| w.light.mode);
    let lighting = json!({
        "setting": setting,
        "compiled_lightmaps_used": setting && compiled_world && mode.is_some_and(|m| m != LightMode::Fullbright),
        "range": mode.map(LightMode::name),
        "fullbright": mode == Some(LightMode::Fullbright),
        "lightmaps": stats.map(|s| s.lightmaps),
        "atlas": stats.map(|s| json!({ "size": s.atlas_size, "height": s.atlas_height, "layers": s.atlas_layers, "bytes": s.bytes })),
        "static_props_vertex_lit": rt.world.as_ref().map(|w| w.light.props.static_props.iter().filter(|p| p.colors.is_some()).count()),
        "world_lights": rt.world.as_ref().map(|w| w.light.props.lights.len()),
        "animated_lightmaps": stats.map(|s| s.animated),
    });
    let cube_stats = state.gfx.as_ref().map(|g| g.renderer.cubemap_stats());
    let sources = state.assets.play_cubemap_sources();
    let cubemaps = json!({
        "setting": state.settings.render.cubemaps,
        "samples": rt.world.as_ref().map(|w| w.cubemaps.len()),
        "built_in_game": sources.map(|s| s.0),
        "drawn_by_editor": sources.map(|s| s.1),
        "drawn_like_game": state.gfx.as_ref().is_some_and(|g| g.renderer.cubemap_lighting().is_some()),
        "faces_waiting": cube_stats.map(|s| s.pending_faces),
        "resolution": cube_stats.map(|s| s.resolution),
    });
    let light_styles = rt.world.as_ref().map(|w| light_styles_json(&w.light.styles, p));
    let trains: Vec<Value> = p
        .trains
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "name": t.name,
                "class": t.class,
                "on_its_path": t.active,
                "moving": t.moving,
                "speed": t.speed,
                "max_speed": t.max_speed,
                "origin": [t.origin.x, t.origin.y, t.origin.z],
                "angles": [t.angles.x, t.angles.y, t.angles.z],
                "passed": t.node,
                "next": t.next,
                "user_control": t.user_control,
            })
        })
        .collect();
    let out = json!({
        "active": p.active || p.start_request,
        "eye": [eye.x, eye.y, eye.z],
        "yaw": state.cameras.cam3d.yaw_deg,
        "pitch": state.cameras.cam3d.pitch_deg,
        "noclip": p.noclip,
        "wireframe": level,
        "wireframe_draws": WIREFRAME_LEVELS[level],
        "compiled_world": compiled_world,
        "lighting": lighting,
        "cubemaps": cubemaps,
        "light_styles": light_styles,
        "trains": trains,
        "area_portals": areas,
    });
    Ok(ToolResult::json(out))
}

fn map_trains(state: &AppState) -> Vec<String> {
    let mut names: Vec<String> = state
        .doc
        .scene()
        .entities()
        .values()
        .filter(|n| hp_ui_viewports::trains::train_kind(&n.entity.classname).is_some())
        .filter_map(|n| n.entity.get("targetname").map(|t| t.trim().to_owned()))
        .filter(|t| !t.is_empty())
        .collect();
    if names.is_empty()
        && state
            .doc
            .scene()
            .entities()
            .values()
            .any(|n| hp_ui_viewports::trains::train_kind(&n.entity.classname).is_some())
    {
        names.push("*".to_owned());
    }
    names.sort();
    names.dedup();
    names
}

fn light_styles_json(table: &hp_editor::limits::world_light::StyleTable, p: &hp_app::runtime::PlayerState) -> Value {
    let running = p.active && !p.light_styles.styles.is_empty();
    let (time, styles, lights) = if running {
        let r = &p.light_styles;
        (r.time, r.styles.clone(), r.lights.clone())
    } else {
        let initial = table.initial();
        let values = table.values(0.0, &initial);
        let styles = table
            .used
            .iter()
            .map(|&s| {
                (s, table.pattern(usize::from(s), &initial).unwrap_or_default().to_owned(), values[usize::from(s)])
            })
            .collect();
        let lights = table
            .lights
            .iter()
            .map(|l| (l.name.clone(), l.style, l.start_on, l.pattern.clone().unwrap_or_else(|| "m".to_owned())))
            .collect();
        (0.0, styles, lights)
    };
    let switchable: Vec<u8> = table.lights.iter().map(|l| l.style).collect();
    json!({
        "time": time,
        "frames_per_second": hp_editor::limits::world_light::STYLE_FPS,
        "styles": styles.iter().map(|(s, pattern, value)| json!({
            "style": s,
            "pattern": pattern,
            "value": value,
            "switchable": switchable.contains(s),
        })).collect::<Vec<_>>(),
        "named_lights": lights.iter().map(|(name, style, on, pattern)| json!({
            "name": name,
            "style": style,
            "on": on,
            "pattern": pattern,
        })).collect::<Vec<_>>(),
        "pending_switches": p.light_requests.iter().map(|(name, on)| json!({ "name": name, "on": on })).collect::<Vec<_>>(),
    })
}

fn views_2d(a: &Args<'_>) -> R<Vec<ViewKind>> {
    Ok(match a.opt_enum("view", VIEWS_2D)?.as_deref() {
        Some("top") => vec![ViewKind::Top],
        Some("front") => vec![ViewKind::Front],
        Some("side") => vec![ViewKind::Side],
        _ => vec![ViewKind::Top, ViewKind::Front, ViewKind::Side],
    })
}

fn cams_json(state: &AppState, kinds: &[ViewKind]) -> Value {
    let mut out = serde_json::Map::new();
    for k in kinds {
        out.insert(view_name(*k).to_owned(), cam2d_json(state.cameras.cam2d(*k)));
    }
    Value::Object(out)
}

fn logical_graph(state: &mut AppState, v: &Value) -> R<ToolResult> {
    use hp_ui_lab::logic::graph::{EdgeState, Graph, NodeFilter};
    let a = args(v, &["filter", "limit"])?;
    let filter = match a.opt_str("filter", 32)? {
        None => NodeFilter::All,
        Some(f) => NodeFilter::from_name(f).ok_or("filter must be \"all\", \"named\" or \"connected\"")?,
    };
    let limit = a.opt_u64("limit", 1, 100_000)?.unwrap_or(2000) as usize;
    let names = state.runtime.helpers.names(&state.doc);
    let g = Graph::build(&state.doc, &names, state.assets.fgd(), filter);
    drop(names);
    let nodes: Vec<Value> = g
        .nodes
        .iter()
        .take(limit)
        .map(|n| {
            json!({
                "id": n.id,
                "class": n.class,
                "name": n.name,
                "icon": n.icon,
                "hidden": !state.doc.is_entity_visible(n.id),
                "outputs": n.outputs,
                "inputs": n.inputs,
                "problems": n.problems,
            })
        })
        .collect();
    let edges: Vec<Value> = g
        .edges
        .iter()
        .filter(|e| e.from < limit && e.to.is_none_or(|t| t < limit))
        .map(|e| {
            json!({
                "from": g.nodes[e.from].id,
                "to": e.to.map(|t| g.nodes[t].id),
                "output": e.output,
                "target": e.target,
                "input": e.input,
                "param": e.param,
                "delay": e.delay,
                "times": e.times,
                "state": match e.state {
                    EdgeState::Valid => "valid",
                    EdgeState::MissingTarget => "missing_target",
                    EdgeState::UnknownOutput => "unknown_output",
                    EdgeState::UnknownInput => "unknown_input",
                },
            })
        })
        .collect();
    Ok(ToolResult::json(json!({
        "filter": filter.name(),
        "total_nodes": g.nodes.len(),
        "total_edges": g.edges.len(),
        "problems": g.problem_count(),
        "nodes": nodes,
        "edges": edges,
    })))
}

fn zoom_2d(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["view", "zoom", "factor", "anchor"])?;
    let kinds = views_2d(&a)?;
    let zoom = a.opt_f64("zoom", Camera2d::MIN_ZOOM, Camera2d::MAX_ZOOM)?;
    let factor = a.opt_f64("factor", 0.01, 100.0)?;
    let anchor = a.opt_vec3("anchor")?;
    let factor_of = |cam: &Camera2d| match (zoom, factor) {
        (Some(z), None) => Ok(z / cam.zoom),
        (None, Some(f)) => Ok(f),
        _ => Err("give zoom or factor".to_owned()),
    };
    for k in &kinds {
        let cam = state.cameras.cam2d_mut(*k);
        let f = factor_of(cam)?;
        let at: DVec2 = anchor.map_or(cam.center, |p| k.to_2d(p));
        cam.zoom_at(f, at);
    }
    Ok(ToolResult::json(cams_json(state, &kinds)))
}

fn pan_2d(state: &mut AppState, v: &Value) -> R<ToolResult> {
    let a = args(v, &["view", "center", "by"])?;
    let kinds = views_2d(&a)?;
    let center: Option<DVec3> = a.opt_vec3("center")?;
    let by = match a.get("by") {
        None => None,
        Some(b) => {
            let bad = || "by must be [right, up] in world units (at most 131072)".to_owned();
            let l = b.as_array().filter(|l| l.len() == 2).ok_or_else(bad)?;
            let x = l[0].as_f64().filter(|x| super::tools::coord_ok(*x)).ok_or_else(bad)?;
            let y = l[1].as_f64().filter(|y| super::tools::coord_ok(*y)).ok_or_else(bad)?;
            Some(DVec2::new(x, y))
        }
    };
    match (center, by) {
        (Some(_), Some(_)) | (None, None) => return Err("give center or by".to_owned()),
        _ => {}
    }
    for k in &kinds {
        let cam = state.cameras.cam2d_mut(*k);
        if let Some(c) = center {
            cam.center = k.to_2d(c);
        }
        if let Some(d) = by {
            let limit = super::tools::MAX_COORD;
            cam.center = (cam.center + d).clamp(DVec2::splat(-limit), DVec2::splat(limit));
        }
    }
    Ok(ToolResult::json(cams_json(state, &kinds)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(state: &mut AppState, name: &str, v: Value) -> Outcome {
        run(state, name, &v).expect("a views tool")
    }

    fn ok(state: &mut AppState, name: &str, v: Value) -> Value {
        match call(state, name, v) {
            Outcome::Done(r) if !r.is_error => r.structured.unwrap(),
            other => panic!("{name}: {other:?}"),
        }
    }

    fn err(state: &mut AppState, name: &str, v: Value) -> String {
        match call(state, name, v) {
            Outcome::Done(r) if r.is_error => r.first_text().to_owned(),
            other => panic!("{name} should fail: {other:?}"),
        }
    }

    #[test]
    fn the_logical_graph_lists_every_entity() {
        let mut s = hp_app::testing::test_state_with_fgd(
            "@PointClass iconsprite(\"editor/light.vmt\") = light : \"\" [ input TurnOff(void) : \"\" ]\n\
             @PointClass = info_player_start : \"\" []\n\
             @SolidClass = trigger_multiple : \"\" [ output OnTrigger(void) : \"\" ]",
        );
        let r = ok(&mut s, "get_logical_graph", json!({}));
        assert_eq!(r["filter"], "all");
        assert_eq!(r["total_nodes"], 3, "{r}");
        let nodes = r["nodes"].as_array().unwrap();
        let light = nodes.iter().find(|n| n["class"] == "light").expect("light");
        assert_eq!(light["icon"], "editor/light");
        assert_eq!(light["inputs"], 1);
        assert!(nodes.iter().any(|n| n["class"] == "info_player_start" && n["name"] == ""));
        assert_eq!(r["edges"][0]["from"], 6);
        assert_eq!(r["edges"][0]["to"], 5);
        assert_eq!(r["edges"][0]["state"], "valid");
        let r = ok(&mut s, "get_logical_graph", json!({ "filter": "named" }));
        assert_eq!(r["total_nodes"], 2);
        let r = ok(&mut s, "get_logical_graph", json!({ "filter": "connected", "limit": 1 }));
        assert_eq!(r["nodes"].as_array().unwrap().len(), 1);
        assert!(err(&mut s, "get_logical_graph", json!({ "filter": "lights" })).contains("filter"));
    }

    #[test]
    fn player_view_wireframe() {
        let mut state = hp_app::testing::test_state();
        let r = ok(&mut state, "player_view", json!({}));
        assert_eq!((r["active"].clone(), r["wireframe"].clone()), (json!(false), json!(0)));
        let r = ok(&mut state, "player_view", json!({ "active": true, "wireframe": 3 }));
        assert_eq!(r["wireframe"], 3);
        assert_eq!(r["active"], true);
        assert!(r["wireframe_draws"].as_str().unwrap().contains("outlines"));
        assert!(state.runtime.lab.player.start_request);
        assert_eq!(state.runtime.lab.player.wireframe, 3);
        assert!(err(&mut state, "player_view", json!({ "wireframe": 4 })).contains("wireframe"));
        assert!(err(&mut state, "player_view", json!({ "wireframe": 1.5 })).contains("wireframe"));
        state.runtime.lab.player.active = true;
        ok(&mut state, "player_view", json!({ "active": false }));
        assert!(!state.runtime.lab.player.active);
    }

    #[test]
    fn player_view_trains() {
        let mut state = hp_app::testing::test_state();
        assert!(err(&mut state, "player_view", json!({ "train": "*" })).contains("no func_tracktrain"));
        let mut cart = hp_vmf::Entity { id: 5, classname: "func_tracktrain".into(), ..Default::default() };
        cart.set("targetname", "Cart");
        let lift = hp_vmf::Entity { id: 6, classname: "func_train".into(), ..Default::default() };
        let map = hp_vmf::Map { entities: vec![cart, lift], ..Default::default() };
        state.doc = hp_editor::Document::from_map(map);
        assert!(err(&mut state, "player_view", json!({ "train": "cart" })).contains("player view"));
        assert!(err(&mut state, "player_view", json!({ "train": "bus", "active": true })).contains("Cart"));
        assert!(err(&mut state, "player_view", json!({ "start": true })).contains("train"));
        ok(&mut state, "player_view", json!({ "train": "cart", "start": true, "active": true }));
        ok(&mut state, "player_view", json!({ "train": "*" }));
        assert_eq!(state.runtime.lab.player.train_requests, [(Some("cart".to_owned()), Some(true)), (None, None)]);
        state.runtime.lab.player.trains = vec![hp_app::runtime::TrainReport {
            id: 5,
            name: "Cart".into(),
            class: "func_tracktrain".into(),
            active: true,
            moving: true,
            speed: 150.0,
            max_speed: 300.0,
            node: "p1".into(),
            next: "p2".into(),
            user_control: true,
            ..Default::default()
        }];
        let r = ok(&mut state, "player_view", json!({}));
        let t = &r["trains"][0];
        assert_eq!(
            (t["name"].clone(), t["moving"].clone(), t["speed"].clone()),
            (json!("Cart"), json!(true), json!(150.0))
        );
        assert_eq!((t["passed"].clone(), t["next"].clone()), (json!("p1"), json!("p2")));
    }

    #[test]
    fn player_view_compiled_lighting() {
        use hp_editor::limits::world::BspWorld;
        let mut state = hp_app::testing::test_state();
        let r = ok(&mut state, "player_view", json!({}));
        assert_eq!(r["lighting"]["setting"], true);
        assert_eq!(r["lighting"]["compiled_lightmaps_used"], false);
        assert!(r["lighting"]["range"].is_null());
        let mut world = BspWorld::default();
        world.light.mode = LightMode::Hdr;
        state.runtime.health.world = Some(std::sync::Arc::new(world));
        let r = ok(&mut state, "player_view", json!({}));
        assert_eq!(r["lighting"]["compiled_lightmaps_used"], true);
        assert_eq!(r["lighting"]["range"], "hdr");
        let r = ok(&mut state, "player_view", json!({ "compiled_lighting": false }));
        assert!(!state.settings.render.player_compiled_lighting);
        assert_eq!(r["lighting"]["compiled_lightmaps_used"], false);
        let mut world = BspWorld::default();
        world.light.mode = LightMode::Fullbright;
        state.runtime.health.world = Some(std::sync::Arc::new(world));
        let r = ok(&mut state, "player_view", json!({ "compiled_lighting": true }));
        assert_eq!(r["lighting"]["fullbright"], true);
        assert_eq!(r["lighting"]["compiled_lightmaps_used"], false);
        assert!(err(&mut state, "player_view", json!({ "compiled_lighting": 1 })).contains("compiled_lighting"));
    }

    #[test]
    fn player_view_light_styles() {
        use hp_editor::limits::world::BspWorld;
        use hp_editor::limits::world_light::{LightEntity, StyleTable};
        let mut state = hp_app::testing::test_state();
        assert!(err(&mut state, "player_view", json!({ "light": "lamp" })).contains("no named"));
        let mut world = BspWorld::default();
        world.light.mode = LightMode::Hdr;
        world.light.styles = StyleTable::from_entities([
            LightEntity { name: "", style: 10, ..Default::default() },
            LightEntity { name: "Lamp", style: 32, pattern: "aaazzz", ..Default::default() },
        ]);
        state.runtime.health.world = Some(std::sync::Arc::new(world));
        let r = ok(&mut state, "player_view", json!({}));
        let styles = &r["light_styles"];
        assert_eq!(styles["styles"][0]["style"], 10);
        assert_eq!(styles["styles"][0]["pattern"], "mmamammmmammamamaaamammma");
        assert_eq!(styles["styles"][0]["value"], 1.0);
        assert_eq!(styles["styles"][1]["switchable"], true);
        assert_eq!(styles["named_lights"][0], json!({ "name": "Lamp", "style": 32, "on": true, "pattern": "aaazzz" }));
        let r = ok(&mut state, "player_view", json!({ "light": "lamp", "on": false }));
        assert_eq!(r["light_styles"]["pending_switches"][0], json!({ "name": "Lamp", "on": false }));
        ok(&mut state, "player_view", json!({ "light": "LAMP" }));
        assert_eq!(
            state.runtime.lab.player.light_requests,
            [("Lamp".to_owned(), Some(false)), ("Lamp".to_owned(), None)]
        );
        assert!(err(&mut state, "player_view", json!({ "light": "nobody" })).contains("Lamp"));
        assert!(err(&mut state, "player_view", json!({ "on": true })).contains("light"));
        state.runtime.lab.player.active = true;
        state.runtime.lab.player.light_styles = hp_app::runtime::LightStyleReport {
            time: 2.5,
            styles: vec![(10, "mmamammmmammamamaaamammma".into(), 0.0), (32, "a".into(), 0.0)],
            lights: vec![("Lamp".into(), 32, false, "aaazzz".into())],
        };
        let r = ok(&mut state, "player_view", json!({}));
        assert_eq!(r["light_styles"]["time"], 2.5);
        assert_eq!(r["light_styles"]["named_lights"][0]["on"], false);
        assert_eq!(r["light_styles"]["styles"][1]["pattern"], "a");
    }

    #[test]
    fn layout_render_mode_zoom_and_pan() {
        let mut state = hp_app::testing::test_state();
        assert!(matches!(call(&mut state, "get_views", json!({})), Outcome::Ui(UiOp::Views)));
        let r = ok(
            &mut state,
            "set_view_layout",
            json!({ "quadrants": ["top", "3d", "SIDE", "front"], "split_x": 0.3, "maximize": "1" }),
        );
        assert_eq!(r["quadrants"], json!(["top", "3d", "side", "front"]));
        assert_eq!(state.ui.maximized_view, Some(1));
        assert!((state.settings.ui.split_x - 0.3).abs() < 1e-6);
        let r = ok(&mut state, "set_view_layout", json!({ "maximize": "none" }));
        assert!(r["maximized"].is_null());
        ok(&mut state, "set_view_layout", json!({ "layout": "default" }));
        assert_eq!(state.settings.ui.quadrants, hp_app::settings::UiSettings::default().quadrants);
        assert!(err(&mut state, "set_view_layout", json!({})).contains("give"));
        assert!(err(&mut state, "set_view_layout", json!({ "layout": "nope" })).contains("no saved layout"));
        assert!(err(&mut state, "set_view_layout", json!({ "layout": "default", "split_x": 0.5 })).contains("alone"));
        assert!(err(&mut state, "set_view_layout", json!({ "quadrants": ["top"] })).contains("quadrants must be"));
        let r = ok(&mut state, "set_view_layout", json!({ "quadrants": ["3d", "LOGICAL", "front", "logical"] }));
        assert_eq!(r["quadrants"], json!(["3d", "logical", "front", "logical"]));
        let logical = QuadrantView::LOGICAL_INDEX;
        assert_eq!(state.settings.ui.quadrants, [0, logical, 2, logical]);
        let rect = egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(300.0, 200.0));
        let shown = [Some((ViewKind::Perspective, rect)), None, None, None];
        let views = views_json(&state, shown, [None, Some(rect), None, None]);
        assert_eq!(views["views"][0]["kind"], "3d");
        assert_eq!(views["views"][1]["kind"], "logical");
        assert_eq!(views["views"][1]["visible"], true);
        assert_eq!(views["views"][1]["rect"], json!([10.0, 20.0, 300.0, 200.0]));
        assert_eq!(views["views"][3]["kind"], "logical");
        assert_eq!(views["views"][3]["visible"], false);
        let bad = json!({ "quadrants": ["3d", "logic", "front", "side"] });
        assert!(err(&mut state, "set_view_layout", bad).contains("\"logical\""));
        ok(&mut state, "set_view_layout", json!({ "layout": "default" }));
        assert!(err(&mut state, "set_view_layout", json!({ "split_y": 2 })).contains("split_y"));

        assert!(matches!(
            call(&mut state, "set_render_mode", json!({ "mode": "Wireframe" })),
            Outcome::Ui(UiOp::RunAction(Action::RenderWireframe))
        ));
        assert!(err(&mut state, "set_render_mode", json!({ "mode": "xray" })).contains("mode must be"));
        assert!(err(&mut state, "set_render_mode", json!({})).contains("mode is required"));

        let before = state.cameras.top.zoom;
        let r = ok(&mut state, "zoom_2d", json!({ "view": "top", "factor": 2 }));
        assert!((state.cameras.top.zoom - (before * 2.0).min(Camera2d::MAX_ZOOM)).abs() < 1e-9, "{r}");
        ok(&mut state, "zoom_2d", json!({ "zoom": 1.0, "anchor": [64, 64, 0] }));
        assert_eq!(state.cameras.front.zoom, 1.0);
        assert!(err(&mut state, "zoom_2d", json!({ "zoom": 1, "factor": 2 })).contains("zoom or factor"));
        assert!(err(&mut state, "zoom_2d", json!({ "zoom": 1000 })).contains("zoom must be"));

        ok(&mut state, "pan_2d", json!({ "view": "front", "center": [10, 20, 30] }));
        assert_eq!(state.cameras.front.center, ViewKind::Front.to_2d(DVec3::new(10.0, 20.0, 30.0)));
        let c = state.cameras.top.center;
        ok(&mut state, "pan_2d", json!({ "view": "top", "by": [16, -8] }));
        assert_eq!(state.cameras.top.center, c + DVec2::new(16.0, -8.0));
        assert!(err(&mut state, "pan_2d", json!({})).contains("center or by"));
        assert!(err(&mut state, "pan_2d", json!({ "by": [1e9, 0] })).contains("by must be"));

        let views = views_json(&state, [None; 4], [None; 4]);
        assert_eq!(views["views"].as_array().unwrap().len(), 4);
        assert!(views["camera_3d"]["position"].is_array());
        assert!(MODE_NAMES.contains(&views["render_mode"].as_str().unwrap()));
        assert_eq!(MODE_NAMES.len(), MODES.len());
    }

    #[test]
    fn render_modes_and_displacements_in_2d() {
        use hp_app::settings::{Disp2dSetting, Disp2dViewSetting, RenderModeSetting};
        let mut state = hp_app::testing::test_state();
        for (name, action, mode) in MODES {
            assert!(MODE_NAMES.contains(name));
            state.settings.view.render_mode =
                if *mode == RenderModeSetting::Flat { RenderModeSetting::Textured } else { RenderModeSetting::Flat };
            assert!(matches!(
                call(&mut state, "set_render_mode", json!({ "mode": name })),
                Outcome::Ui(UiOp::RunAction(a)) if a == *action
            ));
            state.settings.view.render_mode = *mode;
            assert_eq!(views_json(&state, [None; 4], [None; 4])["render_mode"], *name);
            assert_eq!(
                ok(&mut state, "set_render_mode", json!({ "mode": name }))["changed"],
                false,
                "{name}: no toggle back"
            );
        }
        for m in [RenderModeSetting::Baked, RenderModeSetting::LightmapGrid, RenderModeSetting::Unshaded] {
            assert!(MODES.iter().any(|(_, _, x)| *x == m), "{m:?}");
        }
        let r = ok(
            &mut state,
            "set_view_layout",
            json!({ "displacements_2d": ["global", "outline", "MESH", "selected_mesh"] }),
        );
        let views = &state.settings.render.displacements_2d_views;
        assert_eq!(views[0], Disp2dViewSetting::Global);
        assert_eq!(views[1], Disp2dViewSetting::Own(Disp2dSetting::Outline));
        assert_eq!(views[2], Disp2dViewSetting::Own(Disp2dSetting::Mesh));
        assert_eq!(views[3], Disp2dViewSetting::Own(Disp2dSetting::SelectedMesh));
        assert_eq!(r["displacements_2d"]["views"][1]["drawn"], "outline");
        assert_eq!(r["displacements_2d"]["views"][0]["choice"], "global");
        assert_eq!(r["displacements_2d"]["source_brushes"], true, "on by default");
        let r = ok(&mut state, "set_view_layout", json!({ "disp_source_brushes": false }));
        assert!(!state.settings.render.disp_source_2d);
        assert_eq!(r["displacements_2d"]["source_brushes"], false);
        state.settings.render.disp_walkable = true;
        let v = views_json(&state, [None; 4], [None; 4]);
        assert_eq!(v["displacements_2d"]["walkability"], true);
        assert_eq!(v["displacements_2d"]["views"][3]["drawn"], "selected_mesh");
        let any = |state: &mut AppState, name: &str, v: Value| super::super::tools::run(state, name, &v);
        let l = match any(&mut state, "list_actions", json!({ "query": "disp" })) {
            Outcome::Done(r) => r.structured.unwrap(),
            other => panic!("{other:?}"),
        };
        let ids: Vec<&str> = l["actions"].as_array().unwrap().iter().filter_map(|a| a["id"].as_str()).collect();
        assert!(ids.contains(&"toggle_disp_walkable") && ids.contains(&"toggle_disp_mesh_2d"), "{ids:?}");
        for id in ["toggle_disp_walkable", "toggle_disp_mesh_2d", "render_baked", "bake_lighting"] {
            assert!(
                matches!(
                    any(&mut state, "run_action", json!({ "action": id })),
                    Outcome::Ui(UiOp::RunAction(a)) if a.id() == id
                ),
                "{id}"
            );
        }
        for (path, value) in [("render.disp_walkable", json!(false)), ("render.displacements_2d", json!("Outline"))] {
            assert!(matches!(
                any(&mut state, "set_setting", json!({ "path": path, "value": value })),
                Outcome::Ui(UiOp::Settings { .. })
            ));
        }
        assert!(err(&mut state, "set_view_layout", json!({ "displacements_2d": ["mesh"] })).contains("4 of"));
        assert!(
            err(&mut state, "set_view_layout", json!({ "displacements_2d": ["a", "b", "c", "d"] })).contains("4 of")
        );
    }
}
