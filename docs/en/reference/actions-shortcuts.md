[Documentation](../README.md) > Reference > Actions and keyboard shortcuts | [Français](../../fr/reference/actions-shortcuts.md) | [Русский](../../ru/reference/actions-shortcuts.md)

# Actions and keyboard shortcuts

Every command of Hammer Pro 1.1.5 is an *action*: one entry shared by the menus, the toolbar, the keyboard shortcuts, the command palette (`Ctrl+P`), the `--script` files and the MCP server (`run_action`, `list_actions`). This page lists all 355 actions, menu by menu, in the order the menus show them.

Columns:

- **Command**: the label in the English interface (with its submenu when it has one).
- **Id**: the stable identifier, used by `[keybindings]` in `settings.toml`, by scripts and by the MCP tool `run_action`.
- **Default**: the shortcut of the default **Hammer Pro** shortcut set.
- **Hammer** / **Hammer++**: the shortcut when that set is chosen in **Settings > Keyboard > Shortcut set**, only where it differs from the Hammer Pro one ("(none)": no shortcut in that set).

With the Hammer Pro set, two Hammer keys also work as extra shortcuts because they collide with nothing: `Shift+Q` (**Clear selection**) and `Shift+R` (**Show grid**). Shortcuts you change yourself always win over the chosen set. See [Keyboard shortcuts](../features/keyboard-shortcuts.md) for rebinding, conflicts and the `F1` reference window.

Keys that are not actions (flying the 3D camera with `W A S D`, mouse gestures, the keys of a tool while it is active, the keys of the player view) are described on the tool pages: [Camera and navigation](../features/camera-navigation.md), [Selection tool](../features/selection-tool.md), [Player view](../features/player-view.md).

`Ctrl+B` (`snap_to_grid`) also starts the [bevel tool](../features/bevel-tool.md) on the brush edge under the cursor in the 3D view, with the Selection, Vertex and Bevel tools.


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
| BSP Decompiler... | `open_bsp` |  |  |  |
| BSP Sniffer... | `bsp_sniffer` |  |  |  |
| Import a model (FBX, OBJ, glTF)... | `import_model` |  |  |  |
| Import models (batch)... | `import_models_batch` |  |  |  |
| Compile and run... | `run_map` | `Ctrl+F9` | `F9` | `F9` |
| Compile now | `compile_now` | `F9` | `Ctrl+F9` | `Ctrl+F9` |
| Cancel compile | `cancel_compile` |  |  |  |
| Batch compile... | `batch_compile` |  |  |  |
| Run the map in game | `launch_game` |  |  |  |
| Reload the map in game | `reload_in_game` |  |  |  |
| Build and save as content... | `build_content` |  |  |  |
| Publish on the Workshop... | `publish_workshop` |  |  |  |
| Exit | `exit` |  |  |  |
| Import heightmap... | `import_heightmap` |  |  |  |
| Export heightmap... | `export_heightmap` |  |  |  |
| Import blend masks... | `import_blend_masks` |  |  |  |
| Export blend masks... | `export_blend_masks` |  |  |  |

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
| Array duplicate... | `array_duplicate` | `Ctrl+Shift+D` |  |  |
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
| Selection element > Whole objects | `select_element_object` | `Shift+4` |  |  |
| Selection element > Vertices | `select_element_vertex` | `Shift+1` |  |  |
| Selection element > Edges | `select_element_edge` | `Shift+2` |  |  |
| Selection element > Faces | `select_element_face` | `Shift+3` |  |  |
| Selection mode > Ignore groups | `toggle_ignore_groups` | `Ctrl+W` |  |  |
| Edge selection > Select edge loop | `edge_select_loop` |  |  |  |
| Edge selection > Select edge ring | `edge_select_ring` |  |  |  |
| Edge selection > Grow selection | `edge_grow` |  |  |  |
| Edge selection > Shrink selection | `edge_shrink` |  |  |  |
| Edge selection > Select boundary | `edge_select_boundary` |  |  |  |
| Properties | `properties` | `Alt+Enter` |  |  |
| Edit color... | `edit_color` | `C` |  |  |
| Set display color... | `set_display_color` |  |  |  |
| Random display colors | `random_display_colors` |  |  |  |
| Reset display colors | `reset_display_colors` |  |  |  |
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
| Grids > Rotate grid 45 degrees | `rotate_grid_45` |  |  |  |
| Grids > Align grid to the selected edge or face | `align_grid_to_selection` |  |  |  |
| Check for problems | `check_problems` | `Alt+P` |  |  |
| Entity report | `entity_report` |  |  |  |
| Go to brush or entity id... | `go_to_brush` | `Ctrl+Shift+G` |  |  |
| Soundscapes... | `edit_soundscapes` |  |  |  |
| Map assets | `show_map_assets` |  |  |  |
| Check for leaks | `check_leaks` |  |  |  |
| Seal the opening with an areaportal | `seal_areaportal` |  |  |  |
| func_detail suggestions... | `show_detail_suggestions` |  |  |  |
| Optimize the map... | `show_optimize` |  |  |  |
| Analyze the optimization | `analyze_optimization` |  |  |  |
| Show visleaves and portals | `toggle_visleaves` |  |  |  |
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
| 3D render mode > 3D Color Correction | `render_color_corrected` | `Alt+9` |  |  |
| 3D render mode > Baked lighting preview | `render_baked` | `Ctrl+F5` |  |  |
| Show models | `toggle_models` |  |  |  |
| Show helpers and sprites | `toggle_helpers` |  |  |  |
| Show tool textures | `toggle_tool_textures` |  |  | `Ctrl+Shift+F2` |
| Background images... | `background_images` |  |  |  |
| Display options > 3D view information | `toggle_view_info` | `O` |  |  |
| Display options > Bounding boxes | `toggle_bounds_overlay` |  |  |  |
| Display options > Real dimensions | `toggle_real_dimensions` | `Alt+M` |  |  |
| Display options > Show overlays | `toggle_overlays` |  |  |  |
| Display options > Show connections | `toggle_connections` |  |  |  |
| Display options > Show editor objects | `toggle_editor_objects` |  |  | `Alt+O` |
| Display options > Show point entities | `toggle_point_entities` |  |  |  |
| Display options > Show entity labels in 2D | `toggle_entity_names` |  |  |  |
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
| Display options > Detail props preview (grass) | `toggle_detail_preview` |  |  |  |
| Display options > Fog volume preview | `toggle_fog_volumes` |  |  |  |
| Display options > Fog volume texture | `toggle_fog_volume_texture` |  |  |  |
| Display options > Fish preview | `toggle_fish_preview` |  |  |  |
| Display options > Swimming fish | `toggle_fish_swim` |  |  |  |
| Display options > Cubemap reflections | `toggle_cubemaps` |  |  |  |
| Display options > Object colors in 2D | `toggle_object_colors_2d` |  |  |  |
| Display options > Object colors in 3D wireframe and flat | `toggle_object_colors_3d` |  |  |  |
| Display options > Tint func_detail brushes | `toggle_detail_tint` |  |  |  |
| Display options > Show visible clusters | `toggle_pvs` |  |  |  |
| Display options > Show nav mesh | `toggle_nav_mesh` |  |  |  |
| Display options > Show particles | `toggle_particles` |  |  |  |
| Display options > Particles of the selection only | `toggle_particles_selected` |  |  |  |
| Display options > Preview volume effects | `toggle_entity_fx` |  |  |  |
| Display options > Player view | `toggle_player_view` | `F6` |  |  |
| Display options > Preview color_correction entities | `toggle_color_correction_preview` |  |  |  |
| Environment preview > Show soundscapes | `toggle_soundscape_view` |  |  |  |
| Environment preview > Hear soundscapes in the player view | `toggle_soundscape_hearing` |  |  |  |
| Hide selected | `hide_selected` | `H` |  |  |
| Hide unselected | `hide_unselected` | `Ctrl+Shift+H` |  |  |
| Unhide all | `unhide_all` | `U` |  |  |
| Use cordons | `toggle_cordon` |  |  |  |
| Move to visgroup... | `move_to_visgroup` |  |  |  |
| Make a visgroup of hidden objects | `hidden_to_visgroup` |  |  |  |
| Center 2D views on selection | `center_2d` | `Ctrl+E` |  |  |
| Center 3D view on selection | `center_3d` | `Ctrl+Shift+E` |  |  |
| Maximize view | `maximize_view` | `Shift+Z` |  |  |
| Maximize window | `toggle_fullscreen` | `F11` |  |  |
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
| Panels > Tool palette in two columns | `tool_palette_two_columns` |  |  |  |
| Panels > Toolbar on two rows | `toolbar_two_rows` |  |  |  |
| Panels > Reset the toolbar | `reset_toolbar` |  |  |  |
| Panels > Sound browser | `show_sound_browser` |  |  | `Shift+F12` |
| Panels > Particle browser | `show_particle_browser` |  |  | `Shift+F11` |
| Panels > Particle preview... | `particle_preview` |  |  |  |
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
| Make func_detail | `convert_to_detail` | `Ctrl+Alt+D` |  |  |
| Select func_detail candidates... | `select_detail_candidates` |  |  |  |
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
| Intersect brushes | `intersect_brushes` | `Ctrl+Shift+K` |  |  |
| Union of brushes | `union_brushes` |  |  |  |
| Inset face... | `inset_face` |  |  |  |
| Interactive inset (I) | `inset_interactive` | `I` |  |  |
| Interactive extrude (E) | `extrude_interactive` | `E` |  |  |
| Nodraw on hidden faces... | `nodraw_hidden_faces` |  |  |  |
| Tie each to its own entity | `tie_each_to_entity` | `Ctrl+Shift+T` |  |  |
| Edit curve | `curve_edit` |  |  |  |
| Edit road | `road_edit` |  |  |  |
| Sidewalk generator... | `road_sidewalk` |  |  |  |
| Curb and gutter generator... | `road_curb` |  |  |  |
| Blend terrain to road... | `road_terrain` |  |  |  |
| Edit road details... | `roadside_edit` |  |  |  |
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
| Place on top of target | `place_on_top` | `Alt+T` |  |  |
| Place against wall | `place_against_wall` | `Alt+L` |  |  |
| Place against wall, facing away | `place_against_wall_away` | `Alt+Shift+L` |  |  |
| Place next to target | `place_next_to` | `Alt+X` |  |  |
| Settle floating objects | `settle_floating` | `Alt+Shift+N` |  |  |
| Fit to ground (supports) | `fit_to_ground` | `Alt+G` |  |  |
| Shape preview | `show_shape_preview` | `Alt+Shift+M` |  |  |
| Create prefab... | `create_prefab` | `Ctrl+R` |  |  |
| Insert prefab at original size | `insert_original_prefab` | `Alt+Shift+C` |  |  |
| Scatter by material... | `scatter_by_material` |  |  |  |
| Physics placement (drop and settle) | `physics_placement` |  |  |  |
| Convert brushes to a prop | `brushes_to_prop` |  |  |  |
| Convert prop to brushes... | `prop_to_brushes` |  |  |  |
| Generate nav mesh | `generate_nav_mesh` |  |  |  |
| Combine props... | `combine_props` |  |  |  |
| Prop options (Propper)... | `propper_options` |  |  |  |
| Shader editor... | `shader_editor` |  |  |  |
| VTF editor... | `vtf_editor` |  |  |  |
| Send to Blender | `send_to_blender` |  |  |  |
| Update from Blender | `pull_from_blender` |  |  |  |
| Stop the Blender live link | `stop_blender_link` |  |  |  |
| Install the Blender add-on | `install_blender_addon` |  |  |  |
| Start the live link with Garry's Mod | `live_link_start` |  |  |  |
| Stop the live link | `live_link_stop` |  |  |  |
| Live link status... | `live_link_panel` |  |  |  |
| Fire input in game... | `live_link_fire_input` |  |  |  |
| Teleport the player to the 3D camera | `live_link_teleport` |  |  |  |
| Send the player back | `live_link_teleport_back` |  |  |  |
| Player follows the 3D camera | `live_link_camera_to_game` |  |  |  |
| 3D camera follows the player | `live_link_camera_from_game` |  |  |  |
| Send all changes to the game again | `live_link_resync` |  |  |  |
| Remove the live link addon from Garry's Mod | `live_link_remove_addon` |  |  |  |
| Sound editor... | `sound_editor` |  |  |  |
| Color correction editor... | `color_correction_editor` |  |  |  |
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
| Slice tool | `tool_slice` | `Shift+Y` |  |  |
| Extrude / carve tool | `tool_extrude` | `Shift+I` |  |  |
| Bevel tool | `tool_bevel` | `Shift+J` |  |  |
| Vertex tool | `tool_vertex` | `Shift+V` |  |  |
| Displacement tool | `tool_displacement` | `Shift+P` |  |  |
| Polygon tool | `tool_polygon` | `Shift+N` |  |  |
| Curve tool | `tool_curve` | `Shift+U` |  |  |
| Road tool | `tool_road` |  |  |  |
| Pipe tool | `tool_pipe` |  |  |  |
| Spline tool | `tool_spline` |  |  |  |
| Cordon tool | `tool_cordon` |  |  |  |
| Path tool | `tool_path` |  |  |  |
| Sprinkle tool | `tool_sprinkle` |  |  |  |
| Split face | `vertex_split_face` | `Ctrl+F` |  |  |
| Scale vertices... | `vertex_scale` | `Alt+E` |  |  |
| Weld selected vertices | `vertex_weld` |  |  |  |
| Weld vertices by distance | `vertex_weld_distance` |  |  |  |
| Remove degenerate faces and edges | `vertex_cleanup` |  |  |  |
| Slide edge loop... | `edge_slide` |  |  |  |
| Rotate vertices... | `vertex_rotate` |  |  |  |
| Smooth displacement loop | `disp_smooth_loop` |  |  |  |
| Set height of displacement loop | `disp_loop_height` |  |  |  |
| Subdivide displacements | `disp_subdivide` |  |  |  |
| Invert displacement alpha | `disp_invert_alpha` |  |  |  |
| Add noise to displacements... | `disp_noise` |  |  |  |
| Fractal noise on displacements... | `disp_fractal_noise` |  |  |  |
| Thermal erosion of displacements... | `disp_thermal_erosion` |  |  |  |
| Hydraulic erosion of displacements... | `disp_hydraulic_erosion` |  |  |  |
| Blend displacement heights | `disp_blend_heights` |  |  |  |
| Limit displacement slope | `disp_limit_slope` |  |  |  |
| Make displacement slope uniform | `disp_uniform_slope` |  |  |  |
| Select displacements by slope | `disp_select_by_slope` |  |  |  |
| Displacement to mesh... | `disp_to_mesh` |  |  |  |
| Generate terrain... | `generate_terrain` |  |  |  |
| Mesh to displacement... | `mesh_to_disp` |  |  |  |
| Conform displacements to mesh... | `disp_conform_mesh` |  |  |  |
| Tile selection with displacements... | `disp_tile_selection` |  |  |  |
| UV lock (vertex tool) | `toggle_uv_lock` |  |  |  |
| Reload content from disk | `reload_content` |  |  |  |
| Rebuild the content index | `rebuild_content_index` |  |  |  |
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
| Check for updates... | `check_for_updates` |  |  |  |
| Entity help | `entity_help` |  |  |  |
| About Hammer Pro | `about` |  |  |  |

## Particle editor (Tools > Content and editors)

| Action | Shortcut | What it does |
|---|---|---|
| `particle_editor` | None | Opens the integrated PCF editor, selecting the particle effect of the selected entity when its file can be loaded without discarding unsaved edits. |

See [Particles](../features/particles.md).

## Command palette only

| Command | Id | Default | Hammer | Hammer++ |
|---|---|---|---|---|
| Gizmo in local space | `toggle_gizmo_local` | `Ctrl+J` |  |  |
| Install the available update | `install_update` |  |  |  |
| Make hint brush | `make_hint_brush` |  |  |  |
| Camera mode: apply the texture to the targeted face | `fly_apply_texture` | `T` |  |  |
| Camera mode: apply the texture to the whole targeted brush | `fly_apply_texture_brush` | `F` |  |  |
| Camera mode: pick the texture of the targeted face | `fly_pick_texture` | `V` |  |  |
| Camera mode: align the targeted face texture to the view | `fly_align_texture` | `N` |  |  |

## Keys inside the clipping tool

While the clipping tool is active, these keys go to the tool first (they win over the map shortcuts with the same key):

| Key | What it does |
|---|---|
| `Shift+X` (again), `Tab`, `Shift+Tab` | Cycle the pieces kept: both, front, back. |
| `Enter` | Clip. |
| `Escape` | Clear the clip points. |
| `C` | Three point mode in the 2D views. |
| `X` | Cycle the axis of the `Shift` + click cut in the 3D view: Auto, X, Y, Z. |
| `O` | Show or hide the measurements. |
| `Shift` + click (3D) | Axis plane through the clicked point. |
| `Ctrl` + click (3D) | Plane of the clicked face. |
| `Alt` (held) | No snapping. |

See [Clip tool](../features/clip-tool.md).

## Keys inside the slice tool

While the slice tool is active, these inputs go to the tool first:

| Key | What it does |
|---|---|
| `Shift+Y` (again), `Tab`, `O` | Switch between left to right and top to bottom. |
| Wheel (3D), `Shift` + wheel (2D) | More or fewer segments, 1 to 64. |
| Left click, `Enter` | Apply the cuts. |
| Right click, `Escape` | Put the position back at the center of the object. |
| `Alt` (held) | No snapping. |

See [Slice tool](../features/slice-tool.md).

## Keys inside the block tool

While a shape is being created (drag, pending box, or the base and height steps in 3D):

| Key | What it does |
|---|---|
| Wheel | One side more or less (cylinder, spike, sphere, arch, torus, pillar), one step (stairs, spiral stairs) or one arch segment (doorway with arch). The view does not zoom meanwhile. |
| `Shift` + wheel | The same count by 4. |
| `Ctrl` + wheel | Unchanged: grid size or zoom of every 2D view. |
| `Enter` | Create the pending shape. |
| `Escape` | Cancel it (the wheel zooms again). |
| `Alt` (held) | No snapping. |

See [Block tool](../features/block-tool.md#changing-the-count-with-the-mouse-wheel).

## Keys inside the pipe tool

While a pipe path is being placed:

| Key | What it does |
|---|---|
| `Shift` + wheel | One side more or less on a round pipe (5 to 48), one slice per bend more or less on a square pipe (1 to 32). The plain wheel keeps zooming. |
| `Enter`, double click | Create the pipe. |
| `Backspace` | Remove the last point. |
| `Escape` | Cancel the path. |

See [Pipe tool](../features/pipe-tool.md).

## Keys inside the spline tool

| Key | What it does |
|---|---|
| Click | Add a point after the selected one (on the surface in the 3D view). |
| Drag a point | Move it (on the surfaces in the 3D view). |
| `Ctrl` + click on the curve | Insert a point there. |
| `Shift` + wheel, `Alt` + wheel | Roll (5 degrees per notch) and width (0.1 per notch) of the selected point. |
| `Enter`, double click | Generate, or apply the changes of an edited spline. |
| `Delete` | Delete the selected point. |
| `Backspace` | Remove the last point of a new spline. |
| `Escape` | Deselect the point, then cancel. |

The actions `spline_edit` (Edit spline) and `spline_bake` (Bake spline to plain objects) are in Tools > Curves and roads. See [Spline tool](../features/spline-tool.md).

## Keys inside the curve tool

| Key | What it does |
|---|---|
| `Shift` + wheel | More or fewer subdivisions while the curve is drawn or adjusted (2 to 4096). The plain wheel keeps zooming. |
| `Enter` | Validate, then create. |
| `Backspace` | Remove the last point. |
| `Escape` | Cancel the curve. |

See [Curve tool](../features/curve-tool.md).

## Related pages

- [Keyboard shortcuts](../features/keyboard-shortcuts.md)
- [Command palette](../features/command-palette.md)
- [Settings reference](settings.md)
- [Script commands](script-commands.md)
- [MCP server](mcp-server.md)

---
[Back to the documentation index](../README.md)
