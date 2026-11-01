[Documentation](../README.md) > Reference > Actions and keyboard shortcuts | [Français](../../fr/reference/actions-shortcuts.md)

# Actions and keyboard shortcuts

Every command of Hammer Pro 1.0.1 is an *action*: one entry shared by the menus, the toolbar, the keyboard shortcuts, the command palette (`Ctrl+P`), the `--script` files and the MCP server (`run_action`, `list_actions`). This page lists all 261 actions, menu by menu, in the order the menus show them.

Columns:

- **Command**: the label in the English interface (with its submenu when it has one).
- **Id**: the stable identifier, used by `[keybindings]` in `settings.toml`, by scripts and by the MCP tool `run_action`.
- **Default**: the shortcut of the default **Hammer Pro** shortcut set.
- **Hammer** / **Hammer++**: the shortcut when that set is chosen in **Settings > Keyboard > Shortcut set**, only where it differs from the Hammer Pro one ("(none)": no shortcut in that set).

With the Hammer Pro set, two Hammer keys also work as extra shortcuts because they collide with nothing: `Shift+Q` (**Clear selection**) and `Shift+R` (**Show grid**). Shortcuts you change yourself always win over the chosen set. See [Keyboard shortcuts](../features/keyboard-shortcuts.md) for rebinding, conflicts and the `F1` reference window.

Keys that are not actions (flying the 3D camera with `W A S D`, mouse gestures, the keys of a tool while it is active, the keys of the player view) are described on the tool pages: [Camera and navigation](../features/camera-navigation.md), [Selection tool](../features/selection-tool.md), [Player view](../features/player-view.md).


## File menu

| Command | Id | Default | Hammer | Hammer++ |
|---|---|---|---|---|
| New map | `new_map` | `Ctrl+N` |  |  |
| Open map... | `open_map` | `Ctrl+O` |  |  |
| Close | `close_map` | `Ctrl+F4` |  |  |
| Close other maps | `close_other_maps` |  |  |  |
| Save | `save_map` | `Ctrl+S` |  |  |
| Save as... | `save_map_as` | `Ctrl+Shift+S` |  |  |
| Save as new version... | `save_as_new_version` |  |  |  |
| Import map into current... | `import_map` |  |  |  |
| Export selection as VMF... | `export_selection` |  |  |  |
| Export a copy... | `export_copy` |  |  |  |
| Export again | `export_again` | `Alt+B` |  |  |
| Export OBJ... | `export_obj` |  |  |  |
| Import props from the game | `import_from_game` |  |  |  |
| Open archive... | `open_archive` |  |  |  |
| Open compiled map (BSP)... | `open_bsp` |  |  |  |
| Import a model (FBX, OBJ, glTF)... | `import_model` |  |  |  |
| Compile and run... | `run_map` | `Ctrl+F9` | `F9` | `F9` |
| Compile now | `compile_now` | `F9` | `Ctrl+F9` | `Ctrl+F9` |
| Cancel compile | `cancel_compile` |  |  |  |
| Batch compile... | `batch_compile` |  |  |  |
| Run the map in game | `launch_game` |  |  |  |
| Reload the map in game | `reload_in_game` |  |  |  |
| Build and save as content... | `build_content` |  |  |  |
| Publish on the Workshop... | `publish_workshop` |  |  |  |
| Exit | `exit` |  |  |  |

## Edit menu

| Command | Id | Default | Hammer | Hammer++ |
|---|---|---|---|---|
| Undo | `undo` | `Ctrl+Z` |  |  |
| Redo | `redo` | `Ctrl+Y` |  |  |
| Find entities... | `find_entities` | `Ctrl+Shift+F` |  |  |
| Find and replace in entities... | `find_replace_text` | `Ctrl+Alt+R` | `Ctrl+Shift+R` | `Ctrl+Shift+R` |
| Cut | `cut` | `Ctrl+X` |  |  |
| Copy | `copy` | `Ctrl+C` |  |  |
| Paste | `paste` | `Ctrl+V` |  |  |
| Paste special... | `paste_special` | `Ctrl+Shift+V` |  |  |
| Delete | `delete` | `Delete` |  |  |
| Duplicate | `duplicate` | `Ctrl+D` |  |  |
| Copy keyvalues | `copy_keyvalues` |  |  |  |
| Paste keyvalues | `paste_keyvalues` |  |  |  |
| Select all | `select_all` | `Ctrl+A` | `Ctrl+Shift+A` | `Ctrl+Shift+A` |
| Clear selection | `select_none` | `Ctrl+Shift+A` | `Shift+Q` | `Shift+Q` |
| Invert selection | `invert_selection` | `Ctrl+I` | `Ctrl+Shift+I` | `Ctrl+Shift+I` |
| Previous face selection | `face_selection_back` | `Alt+Z` |  |  |
| Next face selection | `face_selection_forward` | `Alt+Y` |  |  |
| Selection mode > Groups | `select_mode_groups` |  |  |  |
| Selection mode > Objects | `select_mode_objects` |  |  |  |
| Selection mode > Solids | `select_mode_solids` |  |  |  |
| Selection mode > Ignore groups | `toggle_ignore_groups` | `Ctrl+W` |  |  |
| Properties | `properties` | `Alt+Enter` |  |  |
| Edit color... | `edit_color` | `C` |  |  |
| Set maximum distance... | `set_max_distance` |  |  |  |

## Map menu

| Command | Id | Default | Hammer | Hammer++ |
|---|---|---|---|---|
| Smaller grid | `grid_smaller` | `[` |  |  |
| Larger grid | `grid_larger` | `]` |  |  |
| Show grid | `toggle_grid` | `Shift+G` | `Shift+R` | `Shift+R` |
| Snap to grid | `toggle_snap` | `Shift+W` |  |  |
| 3D grid on faces | `toggle_grid_3d` | `P` |  |  |
| Grids > Global grid | `grid_mode_global` |  |  |  |
| Grids > One grid per view | `grid_mode_per_view` |  |  |  |
| Grids > Custom grid | `grid_mode_custom` |  |  |  |
| Grids > Grid of the selection | `grid_mode_selection` |  |  |  |
| Grids > Grid from selection | `grid_from_selection` |  |  |  |
| Grids > Grid from face | `grid_from_face` |  |  |  |
| Grids > Attach the grid to the selection | `attach_grid` |  |  |  |
| Grids > Detach the grid of the selection | `detach_grid` |  |  |  |
| Grids > Grid manager... | `grid_manager` |  |  |  |
| Check for problems | `check_problems` | `Alt+P` |  |  |
| Entity report | `entity_report` |  |  |  |
| Go to brush or entity id... | `go_to_brush` | `Ctrl+Shift+G` |  |  |
| Soundscapes... | `edit_soundscapes` |  |  |  |
| Map assets | `show_map_assets` |  |  |  |
| Check for leaks | `check_leaks` |  |  |  |
| Seal the opening with an areaportal | `seal_areaportal` |  |  |  |
| func_detail suggestions... | `show_detail_suggestions` |  |  |  |
| Compare with another map... | `compare_maps` |  |  |  |
| Insert entity gallery | `entity_gallery` |  |  |  |
| Load leak pointfile... | `load_pointfile` |  |  |  |
| Unload pointfile | `unload_pointfile` |  |  |  |
| Load portal file... | `load_portal_file` |  |  |  |
| Unload portal file | `unload_portal_file` |  |  |  |
| Portals and visibility... | `show_portal_viewer` |  |  |  |
| Build portals (geometry only) | `build_portals` | `Ctrl+Shift+P` |  |  |
| Load nav mesh... | `load_nav_mesh` |  |  |  |
| Map properties... | `map_properties` |  |  |  |
| Map information | `map_info` |  |  |  |
| Lighting > Rebuild cubemaps | `rebuild_cubemaps` |  |  |  |
| Lighting > Write cubemaps into the compiled map | `write_cubemaps` |  |  |  |
| Lighting > Bake lighting | `bake_lighting` |  |  |  |
| Lighting > Bake around the camera | `bake_around_camera` |  |  |  |
| Lighting > Stop baking | `stop_bake` |  |  |  |
| Lighting > Select faces with a tiny lightmap scale | `select_tiny_lightmap_faces` |  |  |  |

## View menu

| Command | Id | Default | Hammer | Hammer++ |
|---|---|---|---|---|
| Top view | `view_top` | `F2` |  |  |
| Side view | `view_side` | `F3` |  |  |
| Front view | `view_front` | `F4` |  |  |
| 3D textured view | `view_3d_textured` | `F5` |  |  |
| 3D flat view | `view_3d_flat` | `Shift+F5` |  |  |
| Next 2D view | `cycle_view_2d` | `Tab` |  |  |
| 2D logical view | `view_logical` |  |  |  |
| Logical view | `show_logical_view` |  |  |  |
| 3D render mode > Textured | `render_textured` | `Alt+1` |  |  |
| 3D render mode > Flat shaded | `render_flat` | `Alt+2` |  |  |
| 3D render mode > Wireframe | `render_wireframe` | `Alt+3` |  |  |
| 3D render mode > Textured + mesh edges | `render_textured_wire` | `Alt+4` |  |  |
| 3D render mode > Lighting preview | `render_lit` | `Alt+5` |  |  |
| 3D render mode > Textured, unshaded | `render_unshaded` | `Alt+6` |  |  |
| 3D render mode > Smoothing groups | `render_smoothing_groups` | `Alt+7` |  |  |
| 3D render mode > Lightmap grid | `render_lightmap_grid` | `Alt+8` |  |  |
| 3D render mode > Baked lighting preview | `render_baked` | `Ctrl+F5` |  |  |
| Show models | `toggle_models` |  |  |  |
| Show helpers and sprites | `toggle_helpers` |  |  |  |
| Show tool textures | `toggle_tool_textures` |  |  | `Ctrl+Shift+F2` |
| Background images... | `background_images` |  |  |  |
| Display options > 3D view information | `toggle_view_info` | `O` |  |  |
| Display options > Bounding boxes | `toggle_bounds_overlay` | `I` |  |  |
| Display options > Show overlays | `toggle_overlays` |  |  |  |
| Display options > Show connections | `toggle_connections` |  |  |  |
| Display options > Show editor objects | `toggle_editor_objects` |  |  | `Alt+O` |
| Display options > Show point entities | `toggle_point_entities` |  |  |  |
| Display options > Show entity names | `toggle_entity_names` |  |  |  |
| Display options > Helpers of every entity | `toggle_helpers_all` |  |  |  |
| Display options > Show ropes | `toggle_ropes` |  |  |  |
| Display options > Simulate ropes | `simulate_ropes` |  |  |  |
| Display options > Render effects | `toggle_render_effects` |  |  |  |
| Display options > Sky preview | `toggle_sky_preview` |  |  |  |
| Display options > 3D skybox | `toggle_skybox_3d` | `Alt+V` |  |  |
| Display options > Collision models | `toggle_collision_models` |  |  |  |
| Display options > Fog preview | `toggle_fog_preview` |  |  |  |
| Display options > Animated materials | `toggle_animated_materials` |  |  |  |
| Display options > Model fade | `toggle_model_fade` |  |  |  |
| Display options > Opaque materials | `toggle_opaque_materials` |  |  |  |
| Display options > Walkable slopes | `toggle_walkable` |  |  |  |
| Display options > Show displacements | `toggle_displacements` |  |  |  |
| Display options > Radius culling | `toggle_radius_culling` |  |  |  |
| Display options > Highlight invalid brushes | `toggle_invalid_brushes` |  |  |  |
| Display options > Use the previewed sky camera in game | `use_sky_camera_in_game` |  |  |  |
| Display options > Displacement mesh in 2D views | `toggle_disp_mesh_2d` |  |  |  |
| Display options > Displacement walkability | `toggle_disp_walkable` |  |  |  |
| Display options > Show nodraw faces | `toggle_nodraw` |  |  |  |
| Display options > Show model collisions | `toggle_collision_only` |  |  |  |
| Display options > Water preview | `toggle_water_preview` |  |  |  |
| Display options > Fog volume preview | `toggle_fog_volumes` |  |  |  |
| Display options > Fog volume texture | `toggle_fog_volume_texture` |  |  |  |
| Display options > Fish preview | `toggle_fish_preview` |  |  |  |
| Display options > Swimming fish | `toggle_fish_swim` |  |  |  |
| Display options > Cubemap reflections | `toggle_cubemaps` |  |  |  |
| Display options > Show visible clusters | `toggle_pvs` |  |  |  |
| Display options > Show nav mesh | `toggle_nav_mesh` |  |  |  |
| Display options > Show particles | `toggle_particles` |  |  |  |
| Display options > Particles of the selection only | `toggle_particles_selected` |  |  |  |
| Display options > Player view | `toggle_player_view` | `F6` |  |  |
| Hide selected | `hide_selected` | `H` |  |  |
| Hide unselected | `hide_unselected` | `Ctrl+Shift+H` |  |  |
| Unhide all | `unhide_all` | `U` |  |  |
| Use cordons | `toggle_cordon` |  |  |  |
| Move to visgroup... | `move_to_visgroup` |  |  |  |
| Make a visgroup of hidden objects | `hidden_to_visgroup` |  |  |  |
| Center 2D views on selection | `center_2d` | `Ctrl+E` |  |  |
| Center 3D view on selection | `center_3d` | `Ctrl+Shift+E` |  |  |
| Maximize view | `maximize_view` | `Shift+Z` |  |  |
| Zoom 2D views in | `zoom_2d_in` | `+` |  |  |
| Zoom 2D views out | `zoom_2d_out` | `-` |  |  |
| Reset view splits | `reset_view_splits` |  | `Ctrl+A` | `Ctrl+A` |
| Shorter draw distance | `far_plane_closer` |  |  |  |
| Longer draw distance | `far_plane_farther` |  |  |  |
| Panels > Texture browser | `show_texture_browser` |  |  |  |
| Panels > Model browser | `show_model_browser` |  |  | `Shift+F10` |
| Panels > Entity properties | `show_entity_properties` |  |  |  |
| Panels > Visgroups | `show_visgroups` |  |  |  |
| Panels > Outliner | `show_outliner` |  |  |  |
| Panels > Undo history | `show_history` |  |  |  |
| Panels > Toolbar | `show_toolbar` |  |  |  |
| Panels > Tool palette | `show_tool_palette` |  |  |  |
| Panels > Status bar | `show_status_bar` |  |  |  |
| Panels > Sound browser | `show_sound_browser` |  |  | `Shift+F12` |
| Panels > Particle browser | `show_particle_browser` |  |  | `Shift+F11` |
| Panels > Lighting | `show_lighting` |  |  |  |
| Panels > Prefabs | `show_prefabs` |  |  |  |

## Tools menu

| Command | Id | Default | Hammer | Hammer++ |
|---|---|---|---|---|
| Find and replace textures... | `replace_textures` | `Ctrl+Shift+R` | (none) | (none) |
| Group | `group` | `Ctrl+G` |  |  |
| Ungroup | `ungroup` | `Ctrl+U` |  |  |
| Tie to entity | `tie_to_entity` | `Ctrl+T` |  |  |
| Move to world | `move_to_world` | `Ctrl+Shift+W` |  |  |
| Carve | `carve` | `Ctrl+Shift+C` |  |  |
| Make hollow | `make_hollow` | `Ctrl+H` |  |  |
| Merge into convex solid | `merge_convex` |  |  |  |
| Snap selection to grid | `snap_to_grid` | `Ctrl+B` |  |  |
| Transform... | `transform_dialog` | `Ctrl+M` |  |  |
| Texture lock | `toggle_texture_lock` | `Shift+L` |  |  |
| Texture scale lock | `toggle_texture_scale_lock` | `Shift+K` |  |  |
| Move to floor | `move_to_floor` | `Alt+N` |  |  |
| Move to ceiling | `move_to_ceiling` | `Alt+U` |  |  |
| Snap each object to grid | `snap_to_grid_individually` | `Ctrl+Shift+B` |  |  |
| Center origins | `center_origins` |  |  |  |
| Random Z rotation | `random_rotate` |  |  |  |
| Random Z rotation options... | `random_rotate_dialog` |  |  |  |
| Movable pivot | `toggle_pivot` |  |  |  |
| Move to floor, aligned to the surface | `move_to_floor_aligned` | `Ctrl+Alt+N` |  |  |
| Move to ceiling, aligned to the surface | `move_to_ceiling_aligned` | `Ctrl+Alt+U` |  |  |
| Merge brushes | `merge_brushes` | `Ctrl+Shift+M` |  |  |
| Nodraw on hidden faces... | `nodraw_hidden_faces` |  |  |  |
| Tie each to its own entity | `tie_each_to_entity` | `Ctrl+Shift+T` |  |  |
| Convert to func_detail | `convert_to_detail` |  |  |  |
| Align > Align left | `align_left` |  |  |  |
| Align > Align right | `align_right` |  |  |  |
| Align > Align top | `align_top` |  |  |  |
| Align > Align bottom | `align_bottom` |  |  |  |
| Align > Align centers horizontally | `align_center_horizontal` |  |  |  |
| Align > Align centers vertically | `align_center_vertical` |  |  |  |
| Flip > Flip X | `flip_x` |  |  |  |
| Flip > Flip Y | `flip_y` |  |  |  |
| Flip > Flip Z | `flip_z` |  |  |  |
| Flip > Flip horizontally | `flip_horizontal` | `Ctrl+L` |  |  |
| Flip > Flip vertically | `flip_vertical` |  | `Ctrl+I` | `Ctrl+I` |
| Insert path point after selection | `path_insert_point` |  |  |  |
| Remove path points and relink | `path_remove_point` |  |  |  |
| Create prefab... | `create_prefab` | `Ctrl+R` |  |  |
| Insert prefab at original size | `insert_original_prefab` | `Alt+Shift+C` |  |  |
| Physics placement (drop and settle) | `physics_placement` |  |  |  |
| Convert brushes to a static prop | `brushes_to_prop` |  |  |  |
| Convert prop to brushes... | `prop_to_brushes` |  |  |  |
| Generate nav mesh | `generate_nav_mesh` |  |  |  |
| Static prop options (Propper)... | `propper_options` |  |  |  |
| Instancing > Create instance from selection... | `create_instance` | `Alt+S` |  |  |
| Instancing > Edit instance | `edit_instance` |  |  |  |
| Instancing > Collapse selected instances | `collapse_instances` |  |  |  |
| Instancing > Collapse all instances... | `collapse_all_instances` |  |  |  |
| Instancing > Instances: hidden | `instances_hidden` |  |  |  |
| Instancing > Instances: tinted | `instances_tinted` |  |  |  |
| Instancing > Instances: normal | `instances_normal` |  |  |  |
| Selection tool | `tool_select` | `Shift+S` |  |  |
| Camera tool | `tool_camera` | `Shift+C` |  |  |
| Entity tool | `tool_entity` | `Shift+E` |  |  |
| Block tool | `tool_block` | `Shift+B` |  |  |
| Texture application | `tool_texture` | `Shift+A` |  |  |
| Apply current texture | `apply_current_texture` | `Shift+T` |  |  |
| Decal tool | `tool_decal` | `Shift+D` |  |  |
| Overlay tool | `tool_overlay` | `Shift+O` |  |  |
| Clipping tool | `tool_clip` | `Shift+X` |  |  |
| Extrude / carve tool | `tool_extrude` | `Shift+I` |  |  |
| Vertex tool | `tool_vertex` | `Shift+V` |  |  |
| Displacement tool | `tool_displacement` | `Shift+P` |  |  |
| Polygon tool | `tool_polygon` | `Shift+N` |  |  |
| Curve tool | `tool_curve` | `Shift+U` |  |  |
| Cordon tool | `tool_cordon` |  |  |  |
| Path tool | `tool_path` |  |  |  |
| Sprinkle tool | `tool_sprinkle` |  |  |  |
| Split face | `vertex_split_face` | `Ctrl+F` |  |  |
| Scale vertices... | `vertex_scale` | `Alt+E` |  |  |
| Subdivide displacements | `disp_subdivide` |  |  |  |
| Invert displacement alpha | `disp_invert_alpha` |  |  |  |
| Add noise to displacements... | `disp_noise` |  |  |  |
| UV lock (vertex tool) | `toggle_uv_lock` |  |  |  |
| Reload content from disk | `reload_content` |  |  |  |
| MCP server | `toggle_mcp_server` |  |  |  |
| Install MCP in Claude, Codex and Gemini | `mcp_install` |  |  |  |
| MCP... | `mcp_panel` |  |  |  |
| Command palette... | `command_palette` | `Ctrl+P` |  |  |
| Options... | `settings` | `Ctrl+,` |  |  |

## Window menu

| Command | Id | Default | Hammer | Hammer++ |
|---|---|---|---|---|
| Layout > Save layout... | `save_layout` |  |  |  |
| Layout > Reset layout | `reset_layout` |  |  |  |
| Console | `show_console` | `` ` `` |  |  |
| Next map | `next_map` | `Ctrl+Tab` |  |  |
| Previous map | `previous_map` | `Ctrl+Shift+Tab` |  |  |

## Help menu

| Command | Id | Default | Hammer | Hammer++ |
|---|---|---|---|---|
| Keyboard shortcuts | `keyboard_shortcuts` | `F1` |  |  |
| Documentation | `open_docs` |  |  |  |
| Report an issue | `report_issue` |  |  |  |
| Entity help | `entity_help` |  |  |  |
| About Hammer Pro | `about` |  |  |  |

## Command palette only

| Command | Id | Default | Hammer | Hammer++ |
|---|---|---|---|---|
| Gizmo in local space | `toggle_gizmo_local` | `Ctrl+J` |  |  |
| Make hint brush | `make_hint_brush` |  |  |  |

## Related pages

- [Keyboard shortcuts](../features/keyboard-shortcuts.md)
- [Command palette](../features/command-palette.md)
- [Settings reference](settings.md)
- [Script commands](script-commands.md)
- [MCP server](mcp-server.md)

---
[Back to the documentation index](../README.md)
