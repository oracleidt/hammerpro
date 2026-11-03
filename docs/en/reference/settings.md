[Documentation](../README.md) > Reference > Settings | [Français](../../fr/reference/settings.md)

# Settings reference

Every option of Hammer Pro 1.0.2 is stored in one TOML file, `settings.toml`, in `%APPDATA%\HammerPro` on Windows (the user configuration folder on other systems). This page lists every table and key of that file, its default value and what it does, with the place in the interface where you change it.

You normally never edit this file by hand: the [Settings window](../features/settings-dialog.md) (**Tools > Options...**, `Ctrl+,`) and the menus change it, and every change is saved at once. Editing it by hand is safe too:

- Every key has a default. Missing keys take their default, unknown keys are ignored, so old and new versions of Hammer Pro can share one file.
- A table is only written when one of its values differs from the defaults, so a fresh file is short.
- If the file cannot be read (a typo in TOML), Hammer Pro starts with the defaults and writes a warning to the log. Close Hammer Pro before editing the file, or your edit is overwritten by the next save.
- The file is written atomically (a temporary file, then a rename).
- The MCP tools `get_settings` and `set_setting` read and change these keys by dotted path (`render.water_preview`, `edit.grid_size`).

Values in the tables use TOML syntax: `true`, `16.0`, `"Dark"`. Enumerations are written with the variant name shown in the table.

## Top level keys

| Key | Default | What it does |
| --- | --- | --- |
| `language` | system language | `"En"` or `"Fr"`. Interface language. On the first run it comes from the system language. **Settings > General**, or the `--lang` argument for one run. |
| `language_chosen` | `false` | Set once the first language choice was made. |
| `recent_files` | `[]` | The maps of **File > Recent maps**, newest first, 12 at most. **Settings > General > Recent maps** clears it. |
| `keybindings` | `{}` | Your shortcut changes: action id = shortcut text (`undo = "Ctrl+Alt+Z"`, `""` for no shortcut). Only changed actions are written. Ids are listed in [Actions and keyboard shortcuts](actions-shortcuts.md). **Settings > Keyboard**. |

## `[view]`: viewports and camera

Mostly on **Settings > Viewports**; the `show_*` toggles are also in the **View** menu.

| Key | Default | What it does |
| --- | --- | --- |
| `render_mode` | `"Textured"` | 3D render mode: `Textured`, `Flat`, `Wireframe`, `TexturedWire`, `Lit`, `Unshaded`, `SmoothingGroups`, `Baked`, `LightmapGrid`. **View > 3D render mode**. |
| `fov_deg` | `75.0` | Field of view of the 3D view, in degrees. |
| `camera_speed` | `1000.0` | Fly speed of the 3D camera, units per second. |
| `camera_fast_multiplier` | `3.0` | Speed factor while `Shift` is held. |
| `mouse_sensitivity` | `0.15` | Mouselook sensitivity. |
| `invert_mouse_y` | `false` | Invert the vertical mouse axis when looking around. |
| `far_plane` | `32768.0` | View distance of the 3D view. **View > Shorter / Longer draw distance** change it too. |
| `msaa` | `true` | Antialiasing of the views. |
| `show_models` | `true` | Draw props as their models. **View > Show models**. |
| `show_helpers` | `true` | Draw entity helpers and sprites. **View > Show helpers and sprites**. |
| `show_tool_textures` | `true` | Draw faces with tool textures. **View > Show tool textures**. |
| `show_fps` | `false` | Show the frame rate. **Settings > General**. |
| `show_3d_grid` | `false` | Grid in the 3D view (Viewports page). |
| `grid_highlight_every` | `8` | Every Nth 2D grid line is highlighted. |
| `show_1024_grid` | `true` | Lines every 1024 units in the 2D views. |
| `background_2d` | `[20, 20, 22]` | Background color of the 2D views (RGB). |
| `background_3d` | `[36, 38, 44]` | Background color of the 3D view (RGB). |
| `selection_color` | `[255, 70, 70]` | Color of the selection (RGB). |
| `zoom_to_cursor` | `true` | The mouse wheel zooms toward the cursor (off: toward the view center). |
| `hover_outline` | `false` | The selection tool outlines the objects under the mouse in the 3D view. |
| `keyboard_layout` | `"Auto"` | Keys that fly the camera: `Auto` (by key position: ZQSD on AZERTY, WASD on QWERTY), `Azerty` (by label: Z Q S D, A down, E up), `Qwerty` (by label: W A S D, Q down, E up). |

## `[edit]`: editing defaults

**Settings > Editing**, the grid keys and the **Map** and **Tools** menus.

| Key | Default | What it does |
| --- | --- | --- |
| `grid_size` | `16.0` | Global grid size, 0.01 to 4096. Toolbar grid field, `[` and `]`. |
| `snap_to_grid` | `true` | Snap to the grid. **Map > Snap to grid** (`Shift+W`). |
| `show_grid` | `true` | Draw the 2D grid. **Map > Show grid** (`Shift+G`). |
| `texture_lock` | `true` | Textures follow moved and rotated brushes. **Tools > Texture lock** (`Shift+L`). |
| `texture_scale_lock` | `false` | Textures scale with scaled brushes. **Tools > Texture scale lock** (`Shift+K`). |
| `select_mode` | `"Groups"` | What a click selects: `Groups`, `Objects`, `Solids`. **Edit > Selection mode**. |
| `default_material` | `"dev/dev_measuregeneric01b"` | Material of new brushes. |
| `default_texture_scale` | `0.25` | Texture scale of new faces. |
| `default_lightmap_scale` | `16` | Lightmap scale of new faces. |
| `default_point_entity` | `"info_player_start"` | Class of the entity tool when nothing else was chosen. |
| `default_brush_entity` | `"func_detail"` | Class used by **Tie to entity**. |
| `undo_levels` | `500` | How many undo steps are kept. |
| `autosave_minutes` | `5` | Autosave interval in minutes (**Settings > General > Autosave**; 0 turns autosave off). |
| `autosave_keep` | `10` | Autosave copies kept per map. |
| `arrow_key_nudge` | `true` | The arrow keys nudge the selection in 2D views. |
| `paste_offset` | `true` | Pasted objects are offset so they do not sit on the originals. |
| `new_brush_color` | none | Display color `[r, g, b]` given to new brushes; none: Hammer's random default color. |
| `undo_restores_selection` | `true` | Selection changes are undo steps: `Ctrl+Z` brings back the previous selection (faces included). They never mark the map as modified. |

## `[ui]`: interface

**Settings > General**, the **View > Panels** menu and the layout itself.

| Key | Default | What it does |
| --- | --- | --- |
| `theme` | `"Dark"` | `Dark` or `Light`. |
| `ui_scale` | `1.0` | Interface scale. |
| `accent_color` | none | Accent color (RGB) of highlights, active tool and selection; absent for the black and white look. |
| `show_texture_browser` | `true` | Textures tab of the right dock. |
| `show_entity_properties` | `true` | Entity tab. |
| `show_visgroups` | `true` | Visgroups tab. |
| `show_outliner` | `false` | Outliner tab. |
| `show_model_browser` | `false` | Models tab. |
| `show_console` | `true` | Console panel. |
| `thumbnail_size` | `96.0` | Size of the texture browser thumbnails. |
| `split_x`, `split_y` | `0.5`, `0.5` | Position of the splitters of the 2x2 views (0 to 1). |
| `quadrants` | `[0, 1, 2, 3]` | View of each quadrant: 0 = 3D, 1 = Top, 2 = Front, 3 = Side, 4 = 2D logical. |
| `background_image` | `""` | Image drawn over the whole window (empty: none). |
| `background_opacity` | `0.15` | Its opacity, 0 to 1. |
| `background_offset` | `[0.0, 0.0]` | Its offset from the centered position, in points. |
| `background_zoom` | `1.0` | Its zoom over the size that just covers the window. |
| `background_hide_3d` | `true` | Do not draw it over the 3D views. |
| `native_frames` | `false` | Use the title bars and frames of the system instead of Hammer Pro's (remote desktop, some window managers). Applied at once. |
| `uv_window`, `portals_window` | none | Last position of the UV editor and portals windows when they are separate windows (written by Hammer Pro). |

## `[game]`: game, content and entity definitions

**Settings > Game**. Changes are applied with **Apply and reload content** or when the window closes. See [Game setup](../getting-started/game-setup.md).

| Key | Default | What it does |
| --- | --- | --- |
| `game` | `"gmod"` | The game maps are made for: an id of the game catalogue (`gmod`, `tf`, `portal2`...), `mod:<folder>` for a mod of `steamapps/sourcemods`, `dir:<path>` for another game folder. Its content is mounted, its FGD loaded and maps compile in its format. See [Target games](../compiler/target-games.md). |
| `game_dirs` | `{}` | Game folders you chose (game id = folder holding `gameinfo.txt`), used instead of the ones found in Steam. |
| `gmod_root` | none | Garry's Mod folder (the one with `garrysmod` and `bin`). Absent: detected at startup. |
| `mount_addons` | `true` | Mount `garrysmod/addons` (folders and `.gma`). |
| `mount_workshop` | `false` | Mount Workshop addons (including legacy `.bin`). |
| `mount_cfg` | `true` | Mount the games of `mount.cfg`. |
| `mount_server_content` | `true` | Mount the content downloaded from servers (`garrysmod/download`, `garrysmod/cache/workshop`). |
| `extra_game_dirs` | `[]` | More game folders to mount. |
| `mount_app_ids` | `[]` | Steam games whose content is mounted, by AppID, in priority order (for example `[240, 440]`). Each is found in the Steam libraries and mounted from its `gameinfo.txt`. See [Game setup](../getting-started/game-setup.md#steam-games-by-appid). |
| `custom_texture_dirs` | `[]` | Custom texture folders: tables with `path`, `prefix` (material name prefix when the folder is not inside a `materials` tree) and `enabled`. See [Custom texture folders](../features/custom-texture-folders.md). |
| `extra_fgds` | `[]` | Extra FGD files loaded after the game FGD. |
| `fgd_override` | none | Replaces the main FGD of the game. |
| `map_dir` | none | Default folder of new maps and of the open dialog. |

## `[compile]`: compiling

**Settings > Compile** and the [Compile window](../compiler/compile-window.md). Keys of removed settings written by old versions (external compiler paths) are ignored.

| Key | Default | What it does |
| --- | --- | --- |
| `preset` | `"Normal"` | `Draft`, `Normal`, `Final` or `Custom` (the step modes below). |
| `lighting` | `"Ldr"` | `Ldr`, `Hdr` or `Both`. |
| `run_bsp`, `run_vis`, `run_rad` | `true` | Steps of the Custom preset: geometry, visibility, lighting. |
| `bsp_mode` | `"Normal"` | Geometry of the Custom preset: `Normal`, `OnlyEnts`, `OnlyProps`. |
| `vis_mode` | `"Normal"` | Visibility of the Custom preset: `Fast` or `Normal`. |
| `rad_mode` | `"Normal"` | Lighting of the Custom preset: `Draft`, `Normal`, `Final`. |
| `pack_custom_content` | `true` | Pack custom content into the BSP (see `pack_policy`). |
| `pack_policy` | `"ReleaseOnly"` | `ReleaseOnly` (only Final compiles and releases pack; iteration builds use the development addon) or `Always`. |
| `dev_addon` | `true` | Iteration builds copy custom content to `garrysmod/addons/hammerpro_dev` instead of packing it. |
| `export_custom_materials` | `false` | Export the custom materials. |
| `copy_to_maps` | `true` | Copy the BSP to the game's `maps` folder. |
| `maps_dir_override` | none | Folder the BSP is copied to (absent: `garrysmod/maps`). |
| `launch_game` | `true` | Run the game after the compile. |
| `launch_via_steam` | `false` | Start the game through Steam. |
| `game_exe_path` | none | Game executable (absent: detected). |
| `game_args` | `"-console -dev"` | Game arguments; the fast start options of the launch popover edit them. |
| `threads` | none | Compiler threads (absent: automatic, one core kept for the editor with `low_priority`). |
| `low_priority` | `true` | Keep a core for the editor. |
| `extra_bsp_args` | `""` | Extra arguments of `hammerpro --bspc`. |
| `extra_rad_args` | `""` | Extra arguments of `hammerpro --rad`. |
| `bsp_flag_choices`, `rad_flag_choices` | `{}` | Flags forced on or off in the options menus: flag = `{ on = true, value = "4" }`. See [Compile options](../compiler/compile-options.md). |
| `save_before_compile` | `false` | Save the map before compiling (the compile always uses a snapshot and never touches the map file). |
| `load_pointfile_on_leak` | `true` | Load the leak path when the map leaks. |
| `stop_on_leak` | `true` | Stop after the geometry when the map leaks. |
| `compile_hidden` | `false` | Compile objects hidden in the editor too. |
| `compile_cordon_only` | `true` | With an active cordon, compile only the cordoned area (as `<map>_cordon`). |
| `cordon_material` | `"TOOLS/TOOLSSKYBOX"` | Not read any more: the sealing material is `visibility.cordon_material`. |
| `smart_recompile` | `true` | Pick the cheapest pipeline from what changed (Draft and Normal). |
| `background_precompile` | `true` with 8 cores or more | Compile in the background while the editor is idle. |
| `progressive` | `false` | Play now: compile Draft, launch, then finish the lighting in the background and swap the map. |
| `progressive_preset` | `"Normal"` | Quality of that background lighting (no control in the window). |
| `history` | `true` | Keep a timing history per map. |
| `static_prop_lighting` | `true` | Allow `-StaticPropLighting` when the quality asks for it. |
| `static_prop_shadows` | `true` | Static props cast shadows (off adds `-nostaticpropshadows`). |
| `expert` | `false` | Expert mode: run the selected profile instead of the preset. |
| `profiles` | `[]` | Expert profiles: `name` and `steps` (`enabled`, `action`, `program`, `args`, `continue_on_error`). |
| `active_profile` | `0` | Index of the selected profile. |
| `compilers` | `[]` | Your compilers and your changes to the detected ones: `id` (`user:N` or `detected:...`), `name`, `mode_flags`, `steps` (`kind`: `bsp`, `vis`, `rad`, `copy`, `game` or `custom`; `enabled`, `exe`, `options` (flag to value, `""` for a switch), `extra`, `args`, `workdir`, `continue_on_error`). See [Choosing a compiler](../compiler/compilers.md). MCP changes it with `create_compiler` and `set_compiler`. |
| `compiler_by_game` | `{}` | The compiler of each game: game id to compiler id. A game that is not listed uses Hammer Pro's compiler. MCP changes it with `select_compiler`. |
| `separate_window` | `true` | The Compile and run window is a window of its own. |
| `window` | none | Last position of that window. |

## `[browser]`: asset browsers

Texture, model, sound and particle browsers, and **Settings > Entities and browsers**.

| Key | Default | What it does |
| --- | --- | --- |
| `favorites` | `[]` | Favorite materials. |
| `recent_materials` | `[]` | Recently used materials (64 at most). |
| `hide_tool_textures` | `false` | Hide tool textures in the texture browser. |
| `hidden_shaders` | `[]` | Shaders whose materials are hidden (`vertexlitgeneric` hides model materials). |
| `only_used_in_map` | `false` | Show only the materials used in the map. |
| `last_filter` | `""` | Last search text. |
| `excluded_folders` | `[]` | Material folders hidden from the texture browser (`folder` or `folder/sub`). |
| `fgd_exclusions` | `true` | Also hide the folders excluded by the FGD (`@MaterialExclusion`). |
| `show_alpha` | `false` | Draw thumbnails of materials with alpha over a checkerboard. |
| `show_sound_browser` | `false` | Sound browser dock tab. |
| `show_particle_browser` | `false` | Particle browser dock tab. |
| `model_grid_view` | `true` | Model browser as a grid of rendered models (off: a list). |
| `tile_size` | `96.0` | Model and particle tiles, 48 to 256 points. |
| `gmod_only` | `false` | Model, sound and particle browsers list only stock Garry's Mod content. |
| `sound_volume` | `0.8` | Volume of sound previews, 0 to 1. |
| `sound_autoplay` | `true` | Selecting a sound plays it. |

## `[selection]`

**Settings > Selection**. See [Selection tool](../features/selection-tool.md).

| Key | Default | What it does |
| --- | --- | --- |
| `clone_keeps_visgroups` | `true` | Clone-dragged copies stay in the visgroups of their originals. |
| `rotation_snap_deg` | `15.0` | Rotation step of handles and gizmo rings, 1 to 180 degrees. |
| `free_rotation_with_alt` | `false` | Alt (instead of Ctrl) rotates without the step. |
| `handle_size` | `7.0` | Size of the 2D handles, 4 to 16 points. |
| `shift_arrow_clones` | `true` | `Shift` + arrow keys clone the selection one grid step away; the one unit nudge is then `Alt` + arrow. |
| `gizmo_local` | `false` | The gizmo of a single point entity follows its angles (`Ctrl+J`). |
| `click_to_zoom` | `false` | The camera tool zooms 2D views on click (`Ctrl` + click zooms out). |
| `stretch_handles` | `true` | Arrows on the faces of the selection in the 3D view stretch it. |
| `random_yaw_min` | `0.0` | **Random Z rotation**: smallest turn, degrees (-360 to 360). |
| `random_yaw_max` | `360.0` | **Random Z rotation**: largest turn, degrees (at most 360 above the smallest). |
| `random_yaw_snap` | `0.0` | **Random Z rotation**: step of the turn, degrees (0: free, up to 180). |
| `random_tilt` | `false` | **Random Z rotation** also tilts pitch and roll a little. |
| `random_tilt_deg` | `3.0` | Largest pitch and roll tilt, degrees (0 to 45). |

## `[brush]`

**Settings > Brushes**.

| Key | Default | What it does |
| --- | --- | --- |
| `reorient_primitives` | `true` | Primitives drawn in the Front or Side view stand along that view's depth axis. |
| `clip_measurements` | `true` | The clipping tool shows the length and angle of the clip line. |
| `clip_current_material` | `false` | Cut faces get the current material (off: they copy the most similar face). |

## `[workflow]`

**Settings > Viewports** (controls), **Settings > General** (maps), **Settings > Keyboard** (shortcut set), **View** and **Window** menus.

| Key | Default | What it does |
| --- | --- | --- |
| `keymap_preset` | `"HammerPro"` | Shortcut set: `HammerPro`, `Hammer`, `HammerPlusPlus`. Your own shortcuts stay on top. |
| `nav_scheme` | `"HammerPro"` | Camera controls: `HammerPro` (right drag looks, WASD flies, middle drag pans), `Hammer` (plus Space + left drag looks, Space + right drag pans, left + right drag moves, arrow keys look), `HammerPlusPlus` (the Hammer controls, middle button looks in 3D). |
| `invert_mouse_x` | `false` | Invert the horizontal mouse axis when looking around. |
| `fly_response` | `0.07` | Seconds the fly camera takes to reach its speed, 0 to 0.5 (0: at once). |
| `grid_step_factor` | `2.0` | Factor of `[` and `]`, 1.25 to 10. With 2, the powers of two from 1/8 to 4096. |
| `ctrl_wheel` | `"GridSize"` | `Ctrl` + wheel in a 2D view: `GridSize` or `ZoomAll2d`. |
| `link_2d_zoom` | `false` | Zooming one 2D view zooms the others. |
| `units` | `"Units"` | Lengths of the status bar: `Units`, `Inches`, `FeetInches`, `Meters` (one unit = 2.54 cm). |
| `show_view_info` | `false` | Frame rate, camera position and angles in the 3D view (`O`). |
| `show_bounds` | `false` | Bounding boxes of visible objects (`I`). |
| `show_toolbar`, `show_tool_palette`, `show_status_bar` | `true` | **View > Panels**. |
| `show_history` | `false` | Undo history tab. |
| `layouts` | `{}` | Saved layouts (**Window > Layout > Save layout...**), 32 at most: `quadrants`, `split_x`, `split_y`, `maximized`, `panels`. |
| `open_in_new_tab` | `true` | Opened maps get their own tab (off: they replace the current map). |

## `[instances]`

**Settings > Instances** and **Tools > Instancing**. See [Instances](../features/instances.md).

| Key | Default | What it does |
| --- | --- | --- |
| `display` | `"Tinted"` | Contents of `func_instance`: `Hidden`, `Tinted`, `Normal`. |
| `folder` | `"instances"` | Folder of new instances, relative to the map folder (or absolute). |
| `collapse_skips_hidden` | `true` | Collapsing leaves out the objects hidden in the instance files. |
| `outline_selected` | `true` | Outline the contents of selected instances and name their file. |
| `show_context` | `true` | Draw the map around an instance being edited, tinted and read only. |

## `[visibility]`

**Settings > Cordons and visgroups**.

| Key | Default | What it does |
| --- | --- | --- |
| `cordon_material` | `"tools/toolsskybox"` | Material of the brushes that seal a cordon compile. |
| `shade_outside_2d` | `true` | Shade the area outside the cordons in 2D views. |
| `shade_opacity` | `0.45` | Opacity of that shade, 0 to 1. |
| `show_cordon_names` | `true` | Cordon names in 2D views. |
| `move_remove_others` | `true` | **Move to visgroup** removes the objects from their other visgroups. |
| `move_hide` | `false` | **Move to visgroup** hides the visgroup afterwards. |

## `[helpers]`

**Settings > Helpers** and **View > Display options**.

| Key | Default | What it does |
| --- | --- | --- |
| `helper_scope` | `"Selected"` | Helpers (radii, cones, lines to targets) of `Selected` entities or `All`. The path links of the `line()` and `cylinder()` helpers (`path_track`, `path_corner`) are drawn for every entity in both modes, like Hammer. |
| `connections` | `"Selected"` | I/O lines: `Off`, `Selected`, `All`. |
| `show_overlays` | `true` | Draw `info_overlay` on their faces. |
| `show_editor_objects` | `true` | Entity icons and helper geometry. |
| `show_entity_names` | `true` | Names of point entities in 2D views. |
| `show_ropes` | `true` | Draw ropes as they hang in game. |
| `simulate_ropes` | `false` | Animate ropes while they settle. |

## `[mesh]`

**Settings > Vertices and displacements**.

| Key | Default | What it does |
| --- | --- | --- |
| `uv_lock` | `false` | Vertex tool: textures follow moved vertices. |
| `live_preview` | `true` | Vertex tool: solids show their new shape while dragging. |
| `four_way_blends` | `true` | Alpha painting can paint the 4 textures of 4-way blend materials. |
| `noise_min`, `noise_max` | `-4.0`, `4.0` | Height range of **Add noise to displacements**. |
| `disp_tool` | `{}` | Every option of the displacement tool panel by name (written by the tool). |

## `[entities]`

**Settings > Entities and browsers**; `[entities.path]` comes from the path tool options.

| Key | Default | What it does |
| --- | --- | --- |
| `io_esc_delimiter` | `true` | New outputs use the ESC separator (parameters may contain commas). |
| `paste_targetname` | `false` | **Paste keyvalues** also pastes the name. |
| `texture_decimals` | `4` | Decimals of the Face Edit fields, 0 to 6. |
| `texture_shift_step` | `1.0` | Drag step of the shift fields. |
| `texture_scale_step` | `0.005` | Drag step of the scale fields. |
| `crowbar_exe` | none | Crowbar executable chosen in the model inspector. |
| `path.class` | `"path_corner"` | Class of new path points. |
| `path.base_name` | `"path"` | Base name of points (`path_1`, `path_2`...). |
| `path.close_loop` | `false` | Ending a path links the last point to the first. |
| `path.relink_on_delete` | `true` | Deleting points links their neighbours. |
| `path.surface_offset` | `0.0` | Height above the surface for points placed in 3D, -1024 to 1024. |

## `[render]`

**Settings > Rendering** and **View > Display options**. See [Render modes and display](../features/render-modes-display.md).

| Key | Default | What it does |
| --- | --- | --- |
| `grid_3d` | `false` | 3D grid on brush faces (`P`). |
| `grid_3d_opacity` | `0.45` | Its opacity. |
| `grid_3d_fade_cells` | `64.0` | It fades out beyond this many cells. |
| `near_plane` | `1.0` | Near clip distance, 0.5 to 16 units. |
| `model_distance` | `0.0` | Models farther than this are boxes (0: no limit). |
| `lod_distances` | `[0.0, 0.0]` | Distances of LOD 1 and LOD 2 for heavy models (0: always full). No control in the window. |
| `models_in_2d` | `"SelectedOnly"` | `Boxes`, `Wireframe`, `SelectedOnly`. |
| `collision_models` | `false` | Collision models over models. **View > Display options > Collision models**. |
| `collision_only` | `false` | Show only collision models. |
| `fill_selected_2d` | `true` | Fill selected faces in 2D views. |
| `fill_selected_2d_opacity` | `0.2` | Opacity of that fill. |
| `camera_in_2d` | `true` | Draw the 3D camera in 2D views. |
| `vertex_dots_2d` | `true` | Brush vertices as dots when zoomed in. |
| `icons_in_2d` | `true` | Entity icons in 2D views. |
| `icons_in_2d_px` | `16.0` | Their size in pixels. |
| `use_visgroup_colors` | `false` | 2D wire colors come from visgroups. |
| `object_colors_2d` | `true` | 2D views draw objects in their display color (see [Display colors](../features/display-colors.md)); off: the classic look. |
| `object_colors_3d` | `false` | The 3D view uses the display colors too in wireframe and flat mode. |
| `highlight_invalid` | `true` | Invalid brushes drawn in red. |
| `render_effects` | `true` | Preview render amount, mode, color, body groups, animation and scale. |
| `sky_preview` | `true` | 2D skybox behind the 3D view. |
| `skybox_3d` | `true` | 3D skybox around `sky_camera` (`Alt+V`). |
| `sky_see_through` | `false` | Leave `toolsskybox` faces out so the sky shows through. |
| `water_preview` | `true` | Water drawn like in game. |
| `water_detail` | `"ReflectAll"` | Like the Water Detail video option of the game: `ReflectAll` (r_waterforceexpensive and r_waterforcereflectentities: expensive water at every distance reflecting the world, props, brush entities, the sky and the 3D skybox drawn again mirrored about its surface), `ReflectWorld` (expensive near the camera, cheap past `$cheapwaterenddistance`, props and brush entities only with `$reflectentities`; `$reflectskyboxonly` and `$reflect3dskybox 0` are honored), `Simple` (cheap water everywhere). |
| `fog_preview` | `true` | Preview the fog of an `env_fog_controller` (Fog Enable, colors, blend, start, end, max density, farz) in the 3D views and the player view. |
| `fog_volume_preview` | `true` | Outline and fill `fog_volume` brushes with their fog (the controller's, or a default look with a warning label); the player view takes the controller fog inside them. |
| `fog_volume_texture` | `true` | Draw the material of `fog_volume` brushes in the 3D view. |
| `fish_preview` | `true` | Draw the fish of `func_fish_pool` entities. |
| `fish_swim` | `true` | The previewed fish swim. |
| `normal_maps` | `true` | Normal maps give surfaces their relief. |
| `player_compiled_lighting` | `true` | The player view uses the lightmaps of the compiled map. |
| `cubemaps` | `true` | `$envmap` materials reflect their cubemap. |
| `cubemaps_auto` | `true` | Cubemaps are drawn again after map changes. |
| `light_shadows` | `16` | Point and spot lights with shadows in the Lit mode, 0 to 64 (the brightest in view first). |
| `lit_indirect` | `true` | The Lit mode gets its sky light and the light bounced by the other surfaces from a background bake of the map with the compiler. |
| `model_lods` | `true` | Every prop is drawn at the level of detail the game picks for it, from the switch distances of its model and the size it takes on the screen. Off: the finest level of every model is always drawn. |
| `game_pixel_grid` | `true` | The 3D views sample their pixels on the same grid as the game (Direct3D 9), half a pixel from the grid Vulkan and Direct3D 12 use, so a screenshot of the preview lines up with a screenshot of the game. |
| `sun_glow` | `true` | The sun sprite and the glow of the `env_sun` entity are drawn in the 3D views like the game (direction, `size`, `overlaysize`, `rendercolor`, `overlaycolor`, `material`, `overlaymaterial`, `HDRColorScale`, the fade over the 40 degrees around the middle of the view and the fade behind what hides the sun). |
| `light_preview_hdr` | `true` | The Lit mode uses the HDR light values (`_lightHDR`, `_lightscaleHDR`, `_ambientHDR`, `_AmbientScaleHDR`). |
| `projected_textures` | `true` | The `env_projectedtexture` entities (the first 16) project their texture and light in the Lit mode, the baked preview and the player view. |
| `sky_occlusion` | `true` | In the Lit mode the `_ambient` sky light only reaches what sees the sky. |
| `tone_mapping` | `"Auto"` | How the lit views (Lit mode, baked preview, the player view with its compiled HDR lighting) turn light into pixels like the game: `Auto` adapts the exposure to the picture like the engine does (`mat_autoexposure_min`, `mat_autoexposure_max`, `mat_tonemap_percent_target`), `Fixed` uses `exposure`, `Off` shows the light as it is. **Tools > Options... > Rendering > HDR tone mapping**. |
| `exposure` | `1.0` | The exposure scale of the `Fixed` mode, 0.05 to 16 (`mat_force_tonemap_scale`). |
| `exposure_min` | `0.5` | The auto exposure never goes under this, 0.05 to 16 (`mat_autoexposure_min`). |
| `exposure_max` | `2.0` | The auto exposure never goes over this, 0.05 to 16 (`mat_autoexposure_max`). |
| `bloom` | `1.0` | Strength of the glow around bright surfaces, 0 to 8 (`mat_bloom_scalefactor_scalar`); 0 turns it off. |
| `map_tonemap` | `true` | The `env_tonemap_controller` of the map sets the exposure range and the bloom scale (`SetAutoExposureMin`, `SetAutoExposureMax`, `SetBloomScale` from its `logic_auto`), like in game. |
| `prop_shadows` | `"Auto"` | What `prop_static` entities cast shadows with in the Lit mode: `Auto` follows the compile (model triangles with `-StaticPropPolys`, collision model otherwise, none with `-nostaticpropshadows`), `Mesh` the model triangles, `Collision` the collision model. |
| `animate_materials` | `true` | Animated materials and sprites play. |
| `model_fade` | `false` | Props fade between their fade distances. |
| `force_opaque` | `false` | Translucent materials drawn opaque. |
| `walkable` | `false` | Tint surfaces too steep to walk on. |
| `radius_culling` | `false` | Hide what is beyond `cull_radius`. |
| `cull_radius` | `4096.0` | Radius of radius culling. |
| `show_displacements` | `true` | Show displacements. |
| `displacements_in_2d` | `true` | Draw displacements in 2D views. |
| `displacements_only` | `false` | Show only displacements. |
| `displacements_2d` | `"Mesh"` | `Mesh`, `SelectedMesh`, `Outline`. |
| `displacements_2d_views` | `["Global", ...]` | Per quadrant override of `displacements_2d`. |
| `disp_source_2d` | `true` | Faint outline of the source brush of displacements in 2D. |
| `disp_walkable` | `false` | Color displacement triangles by walkability. |
| `occlusion_culling` | `true` | Skip chunks hidden behind closer geometry. |
| `selection_outline` | `false` | Outline of the selection through walls. |
| `grid_dotted` | `false` | 2D grid as dots. |
| `grid_highlight_64` | `false` | Brighter lines every 64 units. |
| `grid_min_px` | `4.0` | Hide 2D grid levels denser than this many pixels. |
| `hide_nodraw`, `hide_clips`, `hide_triggers`, `hide_hints` | `false` | Tool texture groups hidden in the 3D view. |
| `max_fps` | `0` | Frame rate cap (0: none). |
| `unfocused_fps` | `30` | Frame rate when the window is not focused (0: no cap). |
| `vsync` | `true` | Wait for the display refresh (after a restart). |
| `vram_budget_mb` | `0` | Graphics memory Hammer Pro may use, in MB (0: automatic, 90% of what the system gives the program). Above 85% the maps of other tabs are freed from the graphics card, above the budget textures are shown at a lower resolution. See [Out of memory](../troubleshooting/out-of-memory.md). |
| `keep_background_maps_on_gpu` | `true` | Keep the maps of other tabs on the graphics card while there is room, for instant tab switches. |

## `[lighting]`

**Settings > Lighting** and the Lighting tab. The options of the preview bake itself (samples, bounces...) belong to each map (its Hammer++ `light_plus` block). See [Lighting preview](../features/lighting-preview.md).

| Key | Default | What it does |
| --- | --- | --- |
| `auto_bake` | `true` | Start a bake when the baked preview is turned on and nothing is baked. |
| `threads` | `0` | Bake threads (0: every core but two), up to 256. |
| `max_luxels_millions` | `16` | Luxels baked at full resolution, 1 to 64 million. |
| `tiny_scale` | `4` | Lightmap scale at or under which a face is tiny, 1 to 64. |
| `budget_styles` | `1` | Light styles counted by the lightmap budget, 1 to 4. |
| `show_budget` | `true` | Lightmap budget gauge in the status bar. |
| `quality` | `"Draft"` | `Draft`, `Normal` or `Final`. |

## `[placement]`

**Settings > Prefabs**. `[placement.sprinkle]` comes from the sprinkle tool options. See [Prefabs](../features/prefabs.md) and [Sprinkle tool](../features/sprinkle-tool.md).

| Key | Default | What it does |
| --- | --- | --- |
| `show_prefabs` | `true` | Prefabs tab. |
| `prefab_library` | none | Folder of new prefabs (absent: `prefabs` in the settings folder). |
| `prefab_dirs` | `[]` | More prefab folders (read only). |
| `game_prefabs` | `true` | Also list the prefabs of the game install. |
| `prefab_tile` | `96.0` | Prefab tiles, 56 to 192 points. |
| `group_prefabs` | `true` | Group the objects of an inserted prefab. |
| `sprinkle.kind` | `""` | Selected sprinkle type. |
| `sprinkle.mode` | `"Add"` | `Add`, `Subtract`, `Replace`, `Overwrite`. |
| `sprinkle.density` | `50.0` | Chance in percent per cell. |
| `sprinkle.radius` | `128.0` | Brush radius, 8 to 4096. |
| `sprinkle.use_grid` | `false` | Use the editor grid instead of the definition grid. |
| `sprinkle.random_yaw` | `true` | Random yaw. |
| `sprinkle.random_offset` | `50.0` | Random move in percent of half a cell. |
| `sprinkle.z_offset` | `0.0` | Distance along the surface normal. |
| `sprinkle.align_to_normal` | `false` | Tilt entities to the surface. |
| `sprinkle.extra_dirs` | `[]` | More folders of sprinkle definitions. |

## `[launch]`

Launch popover of the compile window and **Settings > Game > Testing in game**. The fast start options are flags of `compile.game_args`. See [Game launch](../features/game-launch.md).

| Key | Default | What it does |
| --- | --- | --- |
| `timing` | `"AtEnd"` | Start the game when the compile finished (`AtEnd`) or when it starts (`AtStart`). |
| `reuse_running` | `true` | Load the map in the game already running. |
| `skip_unused_content` | `false` | Skip Workshop and addons when the map uses nothing from them. |
| `load_report` | `false` | Tell how long the game took to load the map (`-condebug`). |
| `spawn_at_camera` | `false` | Place the player at the editor camera (needs the bridge addon). |
| `args_version` | `0` | Internal: version of the one time update of the game arguments. |

## `[content]`

The **Build and save as content** window and the Map assets tab. See [Build and save as content](../features/build-content-release.md) and [Protect your BSP](../features/bsp-protection.md).

| Key | Default | What it does |
| --- | --- | --- |
| `mode` | `"WorkshopAddon"` | `WorkshopAddon` (BSP, content tree and `.gma`), `SelfContained` (everything in the BSP), `ServerKit` (self-contained BSP and a FastDL tree). |
| `compile_first` | `true` | Run a Final compile first. |
| `compress_pakfile` | `false` | LZMA compress the pakfile (Garry's Mod prints a warning per entry). |
| `lzma_level` | `6` | LZMA preset, 0 to 9. |
| `pack_workshop` | `true` | Ship content from Workshop items (else listed as required items). |
| `pack_mounted` | `false` | Ship content of `mount.cfg` games. |
| `keep_cubemaps` | `true` | Copy the built cubemaps of the previous BSP. |
| `game_session` | `false` | Build cubemaps and the nav mesh in game. |
| `write_gma` | `true` | Write the `.gma`. |
| `open_folder` | `false` | Open the output folder when done. |
| `protect` | `false` | Protect your BSP. |
| `protect_watermark` | `""` | Watermark of the protected copy. |
| `protect_strip_original_faces` | `true` | Empty the original faces lump. |
| `protect_lock` | `false` | Server lock. |
| `protect_lock_key` | `""` | Secret key of the server lock (only its hash goes in the map). |
| `protect_lock_block` | `false` | Block play on an unauthorized server. |
| `protect_randomize` | `true` | Material randomizer. |
| `protect_shuffle_lumps` | `true` | Random physical order of the lumps. |
| `protect_junk_gaps` | `true` | Random junk gaps between the lumps. |
| `protect_decoys` | `true` | Unreferenced decoy blocks. |
| `protect_scramble_header` | `true` | Random header fields the game ignores. |
| `protect_shuffle_game_lumps` | `true` | Shuffled game lump directory. |
| `protect_random_pak` | `true` | Pakfile at a random position. |
| `protect_reorder_tables` | `true` | Permute planes, vertexes, edges, texinfo and texdata. |
| `protect_reorder_world` | `true` | Permute brushes and faces (the leaf lists stay in order). |
| `protect_layout_seed` | `""` | Seed of the structure scrambler (empty: random at each compile). |
| `show_map_assets` | `false` | Map assets tab. |
| `collect_folder` | `""` | Last output folder of **Collect**. |

## `[health]`

**Settings > Map health**. See [Check for problems](../features/check-problems.md) and [Leaks and pointfiles](../features/leaks-pointfiles.md).

| Key | Default | What it does |
| --- | --- | --- |
| `live_checks` | `true` | Check the map for problems in the background. |
| `live_leak_check` | `true` | Instant leak check in the background. |
| `show_leak_path` | `true` | Draw the path of that leak. |
| `limit_profile` | `"auto"` | Limits of the estimates: `auto` (the game of the map) or a profile id. |
| `visible_only` | `false` | List only problems of visible objects. |
| `portal_width` | `1.0` | Portal line width, 1 to 6 pixels. |
| `leaf_grid` | `false` | Draw the 1024 unit grid leaves are split on. |
| `leaf_width` | `1.0` | Its line width, 1 to 6. |
| `offer_portals` | `true` | Offer to load the new portal file after a compile. |
| `portal_heat` | `false` | Color portals by render cost. |
| `portal_xray` | `true` | Show portals through walls in 3D. |

## `[lab]`

**Settings > Nav mesh and particles** and the Propper and Unpropper options. See [Navigation mesh](../features/nav-mesh.md), [Particles](../features/particles.md), [Propper](../features/propper.md).

| Key | Default | What it does |
| --- | --- | --- |
| `show_nav_mesh` | `false` | Draw the nav mesh. |
| `nav_connections` | `true` | Draw connections between areas. |
| `nav_fill` | `true` | Fill areas in 3D. |
| `show_particles` | `true` | Play `info_particle_system` effects. |
| `particles_selected_only` | `false` | Only the selected systems. |
| `unprop_detail` | `true` | **Convert prop to brushes** puts the brushes in a `func_detail`. |
| `unprop_keep_prop` | `false` | Keep the prop. |
| `unprop_shape` | `"auto"` | `auto`, `collision` or `render`. |
| `model_lod.enabled` | `true` | Make levels of detail for generated models. |
| `model_lod.levels` | `4` | Levels below the full model, 3 to 6. |
| `model_lod.reduction` | `0.5` | Part of triangles kept per level, 0.2 to 0.9. |

## `[sniffer]`

The [BSP Sniffer](../features/bsp-sniffer.md) panel (File > BSP Sniffer...). Only the keys that differ from the default are written.

| Key | Default | What it does |
| --- | --- | --- |
| `address` | `""` | Server as `ip:port`. |
| `authorized` | `false` | You confirm you may test the server (needed to start). |
| `consent_seen` | `false` | The warning dialog was accepted. |
| `output_dir` | `""` | Output folder (empty: the `sniffer` cache folder). |
| `file_name_template` | `"{map}_sniffed"` | File name; tokens `{map}` `{ip}` `{port}` `{date}` `{time}` `{datetime}`. |
| `gmod_dir` | `""` | Garry's Mod folder (empty: automatic). |
| `launch_options` | windowed, 800 x 450, low settings, muted | Added to the Steam launch. |
| `insecure` | `"auto"` | `auto`, `always` or `never` for `-insecure`. |
| `game_bits` | `"auto"` | `auto`, `64` or `32`: which game of the install is started and read. |
| `use_map_file` | `false` | Also read the map file of the client to cross check the memory and fill in what it lacks. Off: no map file is opened. |
| `read_ldr_lighting` | `true` | Read the other lighting set with a second map load in LDR. |
| `attach_running` | `false` | Use a game that already runs. |
| `process_timeout_secs` | `180` | Wait for the game process. |
| `load_timeout_secs` | `900` | Wait for the map. |
| `settle_secs` | `6` | The map must stay identical this long. |
| `poll_secs` | `3` | Polling interval. |
| `save_models`, `save_materials`, `save_sounds`, `save_other` | `true` | What content to save. |
| `only_non_stock` | `true` | Skip files of the base game. |
| `keep_bsp` | `true` | Keep the recovered BSP. |
| `kill_game` | `true` | Close the game that was started. |
| `auto_disconnect` | `true` | Disconnect when the game stays open. |
| `open_when_done` | `true` | Open the VMF when done. |
| `decompile.*` | `true` | `brushes`, `displacements`, `entities`, `brush_entities`, `static_props`, `overlays`, `cubemaps`, `areaportals`, `occluders`, `group_details`. |

## `[grid]`

**Map > Grids**, the toolbar grid menu and the grid manager. See [Grids](../features/grids.md).

| Key | Default | What it does |
| --- | --- | --- |
| `mode` | `"Global"` | `Global`, `PerView`, `Custom`, `Selection`. |
| `custom` | `""` | Name of the custom grid of the Custom mode. |
| `per_view` | 16 everywhere | `perspective`, `top`, `front`, `side`: each with `size`, `origin`, `major_every`. |
| `presets` | `[]` | Custom grids offered in every map (64 at most): `name`, `step` (per axis), `origin`, `angles`, `major_every`, optional `bounds` and `color`. |
| `object_grids` | `true` | Editing an object that carries a grid snaps to it. |
| `plane_3d` | `true` | Draw the plane of a custom or per view grid in 3D. |

## `[mcp]` and `[discord]`

| Key | Default | What it does |
| --- | --- | --- |
| `mcp.start_with_app` | `false` | Switch the MCP server on when Hammer Pro starts. **Settings > MCP**. See [MCP server](mcp-server.md). |
| `discord.enabled` | `true` | Show the activity on Discord. |
| `discord.show_map` | `true` | Show the map name. |
| `discord.show_elapsed` | `true` | Show the time spent. |
| `discord.show_tool` | `true` | Show the active tool. |
| `discord.application_id` | `""` | Your own Discord application id, 17 to 20 digits; empty: Hammer Pro's own application. |
| `discord.project_url` | `""` | Link of the button (no button when empty). |

See [Discord Rich Presence](../features/discord.md).

## `[scan]`

| Key | Default | What it does |
| --- | --- | --- |
| `scan.use_index` | `true` | Keep the saved content index (`content_index.bin` in the cache folder) so that launches only scan the folders that changed. **Settings > Game > Content scan**. |
| `scan.mode` | `"auto"` | `"auto"` (one folder at a time on a hard disk, parallel on SSD and NVMe, per volume), `"parallel"` or `"sequential"`. Applied at the next content load. |

See [Content index and scan](../features/content-index.md).

## `[update]`

| Key | Default | What it does |
| --- | --- | --- |
| `update.check_on_startup` | `true` | Look for a newer release in the background when Hammer Pro starts. **Settings > Updates**. |
| `update.show_dialog` | `true` | Show the update window when a newer release is found. Turning it back on also clears `update.skipped_version`. **Settings > Updates**. |
| `update.skipped_version` | `""` | Tag of the release the user chose not to be asked about (**Don't show this again**), for example `"v1.0.3"`. Empty: none. |

See [Updates](../features/updates.md).

## Related pages

- [Settings window](../features/settings-dialog.md)
- [Files and folders](files-and-folders.md)
- [Actions and keyboard shortcuts](actions-shortcuts.md)

---
[Back to the documentation index](../README.md)
