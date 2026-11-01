[Documentation](../README.md) > Reference > Script commands | [Français](../../fr/reference/script-commands.md)

# Script commands

`hammerpro.exe --script file.txt` plays a text file of commands in the editor: mouse moves and clicks, keys, actions, tool options, screenshots and checks. It is made for automated end-to-end tests and for reproducing a bug step by step. A script run is an automated run: default settings, nothing written to your settings, logs or game (see [Command line](command-line.md)).

## File format

- One command per line. Blank lines are skipped.
- `#` starts a comment: everything after it on the line is ignored (so a `type` text cannot contain `#`).
- Arguments are separated by spaces. The commands that take a path or a text (`open`, `save`, `screenshot`, `type`, `log`...) read the rest of the line, spaces included.
- A line that cannot be read stops the run before it starts with `script line N: cannot parse ...` and exit code 2.
- Each command prints `script: ...` on stderr when it runs, which makes the output easy to follow.
- The run does not close the window at the end of the script: finish with `exit`.

## Positions (POS)

| Form | Meaning |
|---|---|
| `X,Y` | Window coordinates in points, from the top left corner of the window. |
| `@vN:FX,FY` | A fraction (0 to 1) of the rectangle of view N. In the default layout `@v0` is the 3D view, `@v1` Top, `@v2` Front, `@v3` Side. `@v1:0.5,0.5` is the center of the Top view. |
| `@top:A,B` | World coordinates in the first Top view (X, Y). Also `@front:A,B` (Y, Z) and `@side:A,B` (X, Z). |

## Buttons and modifiers

- Mouse buttons: `left` (default), `right`, `middle`.
- Modifiers (MODS): `ctrl`, `shift`, `alt`, joined with `+`, for example `ctrl+shift`.
- Key combos (COMBO): modifiers and one key joined with `+`, for example `Ctrl+Shift+S`, `Enter`, `Escape`, `Delete`, `F9`, `Space`, `ArrowUp`, `A`. Key names are the egui key names.
- Modifiers held with `keydown` apply to every following event until `keyup`.

## Commands

### Timing

| Command | What it does |
|---|---|
| `wait N` | Waits N frames. Frames, not time: under heavy load a run can drop to a few frames per second, so be generous. |
| `idle MS` | Lets MS milliseconds of real time pass without forcing frames (the editor only draws the frames it asks for itself, like when nobody touches the mouse), then prints how many frames were drawn. |

### Mouse

| Command | What it does |
|---|---|
| `move POS` | Moves the pointer. |
| `click POS [button] [mods]` | Press and release. |
| `dclick POS [button] [mods]` | Double click. |
| `drag POS POS [button] [steps] [mods]` | Press at the first position, move to the second in `steps` moves (default 8), release. The modifiers stay held for the whole drag. |
| `press POS [button] [mods]` | Press a mouse button only: it stays down across the following commands (`idle`, `move`...). |
| `release POS [button] [mods]` | Release a mouse button pressed with `press`. |
| `scroll POS DY [mods]` | Mouse wheel. Positive DY zooms in or moves forward. `scroll @v1:0.5,0.5 1 ctrl` changes the grid size. |

### Keyboard

| Command | What it does |
|---|---|
| `key COMBO` | Press and release a key, for example `key Ctrl+Shift+S`, `key Enter`. `Ctrl+C` and `Ctrl+X` also send the copy and cut events text fields listen to. |
| `keydown COMBO` | Press keys and modifiers and keep them down, for example `keydown Shift`, `keydown Space` (pan with a following drag). |
| `keyup COMBO` | Release keys and modifiers held with `keydown`. |
| `hold KEY FRAMES` | Hold one key down for some frames (default 30), for example to fly the 3D camera. |
| `type TEXT` | Types the text into the focused field. |

### Editor

| Command | What it does |
|---|---|
| `action ID` | Runs an action by its id, like its menu entry or shortcut (`compile_now`, `toggle_grid`...). The ids are listed in [Actions and shortcuts](actions-shortcuts.md). An unknown id prints `unknown action`. |
| `tool NAME` | Switches tool: `select`, `camera`, `entity`, `block`, `texture`, `decal`, `overlay`, `clip`, `extrude`, `vertex`, `path`, `polygon`, `curve`, `sprinkle`, `displacement`, `cordon`. |
| `tool_option KEY VALUE` | Sets an option of the active tool (see Tool options below). The value is one word: use commas inside lists (`custom_axis 0,0,1`). An unknown key or a bad value prints `tool ... has no option ...`. |
| `open PATH` | Opens a map and waits until it is loaded (up to 120 seconds). A map that is already open gets its tab shown. |
| `save PATH` | Saves the map to PATH, without a dialog. |
| `material NAME` | Sets the current material (the one new brushes and Apply get). |
| `select_id ID` | Selects the entity with this id, or else the solid with this id. |
| `grid SIZE` | Sets the grid size. |
| `camera X Y Z YAW PITCH` | Places the 3D camera. |
| `player X Y Z YAW PITCH` | In the player view: puts the eyes there, looking that way (pitch up is positive), in noclip so the player stays. |
| `setting PATH VALUE` | Changes a setting by its dotted path, like the MCP tool `set_setting`. VALUE is JSON or a plain string: `setting render.normal_maps false`. The keys are in [Settings reference](settings.md). |
| `custom_folder PATH [\| prefix]` | Adds a custom texture folder (enabled) and remounts the content. Without a prefix, one is suggested from the folder name. |
| `import_model PATH` | Opens the model import dialog with a file (FBX, OBJ, glTF, GLB). |
| `import_model_now PATH` | Imports the model with the default options. |
| `import_model_scaled SCALE PATH` | Imports the model with this scale (greater than 0). Script runs write the model into a temporary content folder, never into the game. |

### Window and look

| Command | What it does |
|---|---|
| `theme dark` / `theme light` | Switches the interface theme. |
| `background OPACITY PATH` | Shows an image over the window (like **Settings > General**, background image), opacity 0 to 1. |
| `window_size W H` | Resizes the main window, in points (its minimum size still applies). |
| `window_maximize on` / `off` | Maximizes or restores the main window. |

### Output and checks

| Command | What it does |
|---|---|
| `screenshot PATH` | Saves a PNG screenshot of the main window. |
| `screenshot_window NAME PATH` | Saves a screenshot of a dialog shown in a window of its own: `compile`, `uv_editor` or `portals` (like the MCP tool `screenshot_window`). |
| `log TEXT` | Prints `script: TEXT` on stderr. |
| `stats` | Prints the frame rate, frame time, worst frame, numbers of solids, faces, entities and selected objects, the asset status, and renderer, particle and cubemap statistics. In the player view it also prints the light styles. |
| `history` | Prints the undo and redo steps, whether an edit is still open, and the bounds of the selection. |
| `assert_selected N` | Stops the run with exit code 3 unless exactly N objects are selected. |
| `assert_tabs N` | Stops the run with exit code 3 unless exactly N maps are open. |
| `exit` | Closes Hammer Pro. |

## Tool options

`tool_option` uses the same option names as the MCP tools `get_tool_options` and `set_tool_options`. Switches accept `on`/`off`, `true`/`false` or `1`/`0`.

### Selection tool (`tool select`)

| Key | Values |
|---|---|
| `handles` | `scale`, `rotate` or `skew`: what the handles of the selection box do in the 2D views. |
| `box` | `touching`, `inside` or `centers`: which objects a box selection in a 2D view takes. |
| `gizmo` | on/off: the transform gizmo in the 3D view. |
| `gizmo_mode` | `move`, `rotate` or `scale`. |
| `gizmo_local` | on/off: the gizmo of a single point entity follows its angles (saved in the settings). |
| `stretch_handles` | on/off: stretch arrows on the faces of the selected brushes in the 3D view (saved in the settings). |

### Block tool (`tool block`)

| Key | Values |
|---|---|
| `kind` | `block`, `wedge`, `cylinder`, `spike`, `sphere`, `arch` or `torus`. |
| `sides` | Sides of round primitives, 3 to 64. |
| `wall` | Arch wall width in units (1 or more). |
| `arc` | Arch arc in degrees, 1 to 360. |
| `start` | Arch start angle in degrees. |
| `rise` | Arch added height in units. |
| `tube_sides` | Torus tube sides, 3 to 64. |
| `ring` | Torus ring width in units (1 or more). |
| `torus_arc` | Torus sweep in degrees (over 360 makes a spiral with a rise). |
| `torus_start` | Torus start angle in degrees. |
| `torus_rise` | Height gained over the torus sweep in units. |
| `reorient` | on/off: primitives drawn in the front or side view or on a wall stand along the view depth (saved). |
| `preview` | on/off: the large shape preview window. |
| `tex_scale` | Texture scale of the new faces, 0.01 to 64. |
| `create_as` | `world`, or a brush entity class (`func_detail`, `trigger_multiple`...). |
| `keep_active` | on/off: stay in the block tool after creating. |

### Curve tool (`tool curve`)

| Key | Values |
|---|---|
| `subdivisions` | Number of brushes along the curve, 2 to 4096. |
| `thickness` | Width of the brushes across the curve, 1 to 1024 units. |
| `height` | Height along the view depth, 1 to 16384 units (also changes the curve being drawn), or `auto`: from the selection like the block tool. |
| `tex_scale` | Texture scale of the new faces, 0.01 to 64. |
| `create_as` | `world`, or a brush entity class (`func_detail`...) holding every brush. |
| `keep_active` | on/off: stay in the curve tool after creating. |

### Path tool (`tool path`)

| Key | Values |
|---|---|
| `class` | Class of the new points: `path_corner`, `path_track`, `path_corner_crash`, `keyframe_rope`... |
| `base_name` | Base name of the points (`path` names them `path_1`, `path_2`...). |
| `loop` | on/off (also yes/no): ending a path links its last point to the first. |
| `relink` | on/off: Delete in this tool links the neighbours of removed points. |
| `offset` | Height above the surface of points placed in the 3D view, -1024 to 1024. |

### Extrude / carve tool (`tool extrude`)

| Key | Values |
|---|---|
| `mode` | `carve` (dig the outline into the brush) or `extrude` (pull it out of the face). |
| `depth` | Depth in units, 0.125 to 16384. |
| `through` | on/off: carve the whole thickness of the brush. |
| `new_faces` | `face` (the clicked face's texture) or `current` (the current material) on the new faces. |

### Texture tool and UV editor (`tool texture`)

| Key | Values |
|---|---|
| `uv_place` | `panel` (docked in the Face edit panel) or `window` (a window of its own). |
| `uv_all_faces` | on/off: the UV editor edits every selected face, or only the edited one. |
| `uv_mode` | `face`, `edge` or `vertex`. |
| `uv_layer` | `base`, `base2`, `bump`, `bump2`, `detail`, `envmask` or `blendmod` (when the material has it). |
| `uv_keep_layers` | on/off: moving the base texture keeps the layers in place on the face. |
| `uv_fit_tiles` | Tiles along the edge for Fit along the edge, 0.25 to 64. |
| `uv_vertices` | Selected vertices of the edited face, indices separated by commas (empty or `none`: nothing). Switches to the vertex mode. |
| `uv_edges` | Selected edges of the edited face (index of the first vertex of each), separated by commas. Switches to the edge mode. |

### Displacement tool (`tool displacement`)

| Key | Values |
|---|---|
| `mode` | `select`, `geometry` (also `paint`), `alpha`, `sculpt` or `tag`. |
| `effect` | Geometry painting: `raise`, `lower`, `smooth` or `set` (also `raise_to`). |
| `axis` | `normal`, `subdiv`, `center` (also `brush_center`), `view`, `x`, `y`, `z` or `custom`. |
| `custom_axis` | Direction of the custom axis, `x,y,z`. |
| `radius` | Brush radius in units, 1 to 8192. |
| `strength` | Raise and lower speed in units per second, 0 to 8192. |
| `smooth` | Smoothing strength, 0 to 1. |
| `target_height` | Height of the set effect, -16384 to 16384. |
| `falloff` | `constant`, `linear` or `smooth`. |
| `spatial` | on/off: 3D distance on the surface (paints walls and across tiles). |
| `line_of_sight` | on/off: only what the camera sees at the brush. |
| `autosew` | on/off: live sewing of the neighbours. |
| `selected_only` | on/off: with displacements selected, only those are painted. |
| `continuous` | on/off: holding the button still keeps painting. |
| `alpha_effect` | `add`, `remove`, `set` or `smooth`. |
| `alpha_value` | Target alpha of the set effect, 0 to 255. |
| `alpha_strength` | Alpha add and remove speed per second, 0 to 16384. |
| `blend_channel` | `alpha` (also `none`) for a two texture blend, or `1` to `4` for a texture of a 4-way blend. |
| `sculpt` | `push`, `carve` or `project`. |
| `push_mode` | `absolute` (units) or `adaptive` (percent of the radius). |
| `sculpt_bounds` | `attenuated` or `additive`. |
| `falloff_position` | Push: percent of the radius that gets the whole offset, 0 to 100. |
| `falloff_final` | Push: percent of the offset the falloff never goes below, 0 to 100. |
| `push_distance` | Push offset in units, 0 to 4096. |
| `push_percent` | Push offset in percent of the radius, 0 to 400. |
| `carve_depth` | Carve depth in units, 0 to 4096. |
| `carve_profile` | `v`, `u`, `flat` or 5 depths from 0 to 1 separated by commas. |
| `project_range` | Project: brushes farther than this are ignored, 1 to 16384. |
| `scale_factor` | Factor of the Scale button, -16 to 16. |
| `tag_kind` | `walkable`, `buildable` or `remove`. |
| `tag_value` | `on`, `off` or `auto`. |
| `mask_grid` | on/off: hide the triangle grid of the selected displacements. |
| `mask_select` | on/off: do not tint the selected displacements. |
| `create` | `2`, `3` or `4`: creates displacements of that power on the selected faces (the Create buttons). |
| `select` | `all`, `adjacent`, `solids` or `none`: the selection buttons of the panel. |
| `cast_shadows` | on/off on the selected displacements. |
| `lightmap_scale` | 1 to 1024 on the selected displacements. |
| `tag_selection` | Any value: applies the current `tag_kind` and `tag_value` to the whole selection. |

## Example

```text
# Draw a 256 unit block in the Top view, check it, compile, save a screenshot
tool block
tool_option kind block
drag @top:0,0 @top:256,256
key Enter
wait 10
assert_selected 1
action compile_now
idle 3000
screenshot C:\temp\after.png
exit
```

## Related pages

- [Command line](command-line.md)
- [Actions and shortcuts](actions-shortcuts.md)
- [Settings reference](settings.md)
- [MCP server](mcp-server.md)
- [Displacement tool](../features/displacement-tool.md)
- [Block tool](../features/block-tool.md)

---
[Back to the documentation index](../README.md)
