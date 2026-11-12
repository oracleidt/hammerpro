[Documentation](../README.md) > Getting started > Interface tour | [Français](../../fr/getting-started/interface-tour.md) | [Русский](../../ru/getting-started/interface-tour.md)

# Interface tour

The Hammer Pro window follows the layout of Valve Hammer and Hammer++ (menus in the same order, tools on the left, four views, properties on the right), so a mapper coming from them finds every button where they expect it. The interface is black and white (dark or light theme): color is kept for your map, textures and models.

## Window frame and title bar

Hammer Pro draws its own window frame: rounded corners, a soft shadow and a title bar that holds the Hammer Pro mark, the menus, the title `<map name> - Hammer Pro` and the minimize, maximize/restore and close buttons. Hovering the title shows the full path of the map ("Not saved yet" for a new map) and "Unsaved changes" when the map is modified.

If the frame misbehaves (remote desktop, some window managers), turn on **Settings > General > Native window frames**: the system's title bar comes back and the menus move to a menu bar under it. See [Window frames](../features/window-frames.md).

## Menu bar

| Menu | What is in it |
|---|---|
| **File** | New map, Open map..., **Recent maps**, Close, Close other maps, Save, Save as..., Save as new version..., **Import** (Import map into current..., Import a model (FBX, OBJ, glTF)..., Import models (batch)..., Import props from the game, Import heightmap..., Open archive..., BSP Decompiler..., BSP Sniffer...), **Export** (Export selection as VMF..., Export a copy..., Export again, Export OBJ..., Export heightmap...), **Compile** (Compile and run..., Compile now, Cancel compile, Batch compile..., Run the map in game, Reload the map in game), **Publish** (Build and save as content..., Publish on the Workshop...), Exit |
| **Edit** | Undo, Redo, Cut, Copy, Paste, Paste special..., Delete, Duplicate, Array duplicate..., Select all, Clear selection, Invert selection, **Selection mode** (Groups, Objects, Solids, Ignore groups), **Selection element** (Whole objects, Vertices, Edges, Faces, Select edge loop, Select edge ring, Grow selection, Shrink selection, Select boundary, Previous face selection, Next face selection), Find entities..., Find and replace in entities..., Properties, Copy keyvalues, Paste keyvalues, Set maximum distance..., **Display colors** (Edit color..., Set display color..., Random display colors, Reset display colors) |
| **Map** | Snap to grid, Show grid, 3D grid on faces, Smaller grid, Larger grid, **Grids** (Global grid, One grid per view, Custom grid, Grid of the selection, Grid from selection, Grid from face, Attach the grid to the selection, Detach the grid of the selection, Grid manager..., Rotate grid 45 degrees, Align grid to the selected edge or face), Check for problems, Check for leaks, Go to brush or entity id..., Entity report, Map assets, Compare with another map..., **Leaks and visibility** (Load leak pointfile..., Unload pointfile, Portals and visibility..., Build portals (geometry only), Load portal file..., Unload portal file, func_detail suggestions..., Seal the opening with an areaportal, Load nav mesh...), Map information, Map properties..., Soundscapes..., Insert entity gallery, **Lighting** (Bake lighting, Bake around the camera, Stop baking, Solo the selected lights, Mute the selected lights, Unmute all lights, Measure the cost of each light, Select faces with a tiny lightmap scale, Rebuild cubemaps, Write cubemaps into the compiled map) |
| **View** | **View type** (Top view, Side view, Front view, 3D textured view, 3D flat view, Next 2D view, 2D logical view, Logical view), **3D render mode** (Textured, Flat shaded, Wireframe, Textured + mesh edges, Lighting preview, Baked lighting preview, Textured, unshaded, Smoothing groups, Lightmap grid, 3D Color Correction), Player view, **Display options** (Show models, Show helpers and sprites, Show tool textures, **Hidden tool textures**, Show point entities, Show editor objects, Helpers of every entity, Show connections, Show overlays, Show displacements, Show nodraw faces, Bounding boxes, Show entity labels in 2D, Show brush centers in 2D, Displacement mesh in 2D views, Object colors in 2D, Object colors in 3D wireframe and flat, Highlight invalid brushes, 3D view information, Background images...), **Environment preview** (Sky preview, 3D skybox, Use the previewed sky camera in game, **3D skybox camera**, Fog preview, Fog volume preview, Fog volume texture, **Previewed fog controller**, Water preview, Detail props preview (grass), Fish preview, Swimming fish, Preview color_correction entities, Cubemap reflections), **Effects** (Render effects, Preview volume effects, Animated materials, Model fade, Opaque materials, Radius culling, Show particles, Particles of the selection only, Show ropes, Simulate ropes), **Diagnostics** (Show visible clusters, Show nav mesh, Collision models, Walkable slopes, Displacement walkability, Show model collisions), Hide selected, Hide unselected, Unhide all, Use cordons, Move to visgroup..., Make a visgroup of hidden objects, Center 2D views on selection, Center 3D view on selection, Maximize view, Maximize window, **Zoom and draw distance** (Zoom 2D views in, Zoom 2D views out, Shorter draw distance, Longer draw distance, Reset view splits), **Panels** (Texture browser, Entity properties, Visgroups, Outliner, Model browser, Sound browser, Particle browser, Particle preview..., Lighting, Prefabs, Undo history, Toolbar, Tool palette, Status bar), **Language** |
| **Tools** | **Active tool** (Selection tool, Camera tool, Entity tool, Block tool, Texture application, Apply current texture, Decal tool, Overlay tool, Clipping tool, Slice tool, Extrude / carve tool, Bevel tool, Vertex tool, Path tool, Polygon tool, Curve tool, Sprinkle tool, Displacement tool, Cordon tool, Road tool, Pipe tool, Physics simulator), Transform..., Snap selection to grid, **Arrange** (Align left, Align right, Align top, Align bottom, Align centers horizontally, Align centers vertically, Flip horizontally, Flip vertically, Flip X, Flip Y, Flip Z, Move to floor, Move to ceiling, Move to floor, aligned to the surface, Move to ceiling, aligned to the surface, Snap each object to grid, Center origins, Movable pivot, Random Z rotation, Random Z rotation options...), Group, Ungroup, Tie to entity, Move to world, Carve, Make hollow, **Brush operations** (Merge brushes, Intersect brushes, Union of brushes, Merge into convex solid, Interactive extrude (E), Interactive inset (I), Inset face..., Convert to func_detail, Tie each to its own entity, Nodraw on hidden faces...), **Vertices and edges** (Split face, Scale vertices..., Weld selected vertices, Weld vertices by distance, Remove degenerate faces and edges, Slide edge loop..., Rotate vertices..., UV lock (vertex tool)), **Displacements** (Subdivide displacements, Smooth displacement loop, Set height of displacement loop, Invert displacement alpha, Select displacements by slope, Blend displacement heights, Limit displacement slope, Make displacement slope uniform, Add noise to displacements..., Fractal noise on displacements..., Thermal erosion of displacements..., Hydraulic erosion of displacements..., Generate terrain..., Tile selection with displacements..., Mesh to displacement..., Conform displacements to mesh..., Displacement to mesh...), **Curves and roads** (Edit curve, Edit road, Edit road details..., Sidewalk generator..., Curb and gutter generator..., Blend terrain to road...), **Textures** (Find and replace textures..., Texture lock, Texture scale lock), **Placement and props** (Create prefab..., Insert prefab at original size, Physics placement (drop and settle), Scatter by material..., Convert brushes to a prop..., Convert prop to brushes..., Prop options (Propper)..., Insert path point after selection, Remove path points and relink, Generate nav mesh), **Instancing** (Create instance from selection..., Edit instance, Collapse selected instances, Collapse all instances..., Instances: hidden, Instances: tinted, Instances: normal), **Content and editors** (Shader editor..., VTF editor..., Sound editor..., Color correction editor..., Particle editor..., Reload content from disk, Rebuild the content index), **Blender and MCP** (Send to Blender, Update from Blender, Stop the Blender live link, Install the Blender add-on, MCP server, MCP..., Install MCP in Claude, Codex and Gemini), Command palette..., Options... |
| **Window** | **Layout** (Save layout..., Reset layout, **Saved layouts**), Console, Next map, Previous map, **Open maps** |
| **Help** | Documentation, Keyboard shortcuts, Entity help, Report an issue, Check for updates..., Setup assistant..., About Hammer Pro |

Names in bold are submenus. Frequent commands sit at the top of each menu, related commands are grouped in submenus and separators split the groups, so no menu is taller than about twenty entries. The [command palette](../features/command-palette.md) shows the menu path of every command (for example "File > Import").

Every command, with its default shortcut, is listed in [Actions and shortcuts](../reference/actions-shortcuts.md). The [command palette](../features/command-palette.md) (`Ctrl+P`) finds any of them by name.

## Toolbar

The toolbar row under the menus reads from left to right like most editors: file actions first, then edit actions, then the map tools, with **Compile**, the main action, at the right end:

1. New map, Open map, Save.
2. Undo, Redo, Cut, Copy, Paste.
3. Show grid, 3D grid on faces, the grid size (smaller, a field where you can type any size from 0.01 to 4096, larger), Snap to grid.
4. Carve, Group, Ungroup, Ignore groups.
5. Hide selected, Hide unselected, Unhide all.
6. Use cordons, Cordon tool, Radius culling.
7. Displacement mesh in 2D views, Displacement walkability, Show nodraw faces, Show model collisions, Texture lock, Texture scale lock.
8. Show helpers and sprites, Show models, Show tool textures, Show editor objects, 3D skybox, Show particles, Simulate ropes.
9. The 3D render mode list, Player view, Check for problems.
10. **Compile** (with its hammer icon), alone at the right end of the first row: it opens the Compile and run window (Hammer's Run Map). While a compile runs, the button keeps its size and shows a progress ring with the percentage (click it to open the Compile and run window) and a stop square on its right end that cancels the compile.

By default the toolbar stays on one row. When the window is too narrow for every group (a small screen, a large interface scale, the longer French or Russian labels), the least used groups go into the **»** button at the end of the row, in this order: the view toggles (8), the displacement and texture lock toggles (7), the cordon group (6), the hide and group buttons (5 and 4), the render mode group (9), then Undo, Redo, Cut, Copy, Paste (2). The groups that remain keep their order, and the **»** menu shows the hidden ones with the same buttons, tooltips and shortcuts; it stays open while you click its buttons. **Compile** is always the last to go.

**Customize the toolbar**: right-click the toolbar (on a button or between them) to open its menu:

- **Wrap on two rows**: when the window is too narrow, the toolbar continues on a second row instead of moving groups into **»** (a wide window still uses one row). Also **View > Panels > Toolbar on two rows** and **Settings > General > Toolbar**.
- **Customize toolbar**: one submenu per group. **Show this group** hides or shows the whole group, a checkbox per button hides or shows that button, **Move left** and **Move right** change the order of the groups. **Compile** can be hidden but always stays at the right end.
- **Reset the toolbar**: back to the default groups, order and one row (also **View > Panels > Reset the toolbar**).
- **Hide the toolbar** (**View > Panels > Toolbar** brings it back).

The choices are saved in the settings (`workflow.toolbar_rows`, `workflow.toolbar_order`, `workflow.toolbar_hidden`). When the toolbar is hidden, or its grid group or texture lock button is hidden, the status bar shows the grid size, snap and texture lock instead.

At the right end of the top bar, the **MCP** pill shows whether the MCP server of this window is on, and its menu installs Hammer Pro in AI apps (see [MCP server](../reference/mcp-server.md)).

Under the toolbar, the **options row** shows the name of the active tool and its options. Tools with large options (Texture application, Displacement...) show them in the **Tool** tab of the right dock instead ("Tool options are in the right panel.").

## Tool palette

The vertical strip along the left edge holds the tools as icons, in groups separated by a line: moving around (Selection, Camera), creating (Entity, Block, Polygon, Curve, Sprinkle, Road, Pipe), editing (Texture application, Apply current texture, Decal, Overlay, Clipping, Slice, Extrude, Bevel, Vertex, Displacement), special tools (Path, Cordon, Physics, then Propper, Unpropper and Send to Blender), and the isolate eye (isolate the selection, or show everything again). The tooltip of each button gives its name and shortcut. When the window is short the buttons get flatter, then the strip scrolls.

The icons are drawn by Hammer Pro in one line style, sharp at every interface scale and in both themes, so each one shows what the tool does: a checkerboard for Texture application, a face with an arrow pointing out for Extrude, a cube corner cut off for Bevel, a curve with its handles for Path, a block split by a plane for Clipping, a block cut in slices for Slice, a sticker with a peeled corner for Decal, stacked layers for Overlay, an arch of blocks for Curve, a terrain for Displacement, a ball falling on the ground for Physics, a brush becoming a barrel for Propper (and the reverse for Unpropper). The toolbar's Carve button shows a block with a bite cut out by another block.

**Two columns**: right-click the palette and choose **Two columns** (or **View > Panels > Tool palette in two columns**, or **Settings > General > Tool palette**) for a wider palette with the buttons side by side, like Photoshop: every button keeps its full size on short windows. **One column** goes back. The choice is saved (`workflow.tool_palette_columns`). The same menu has **Hide the tool palette**.

## The four views

The center of the window is a 2x2 grid of views: the 3D view at top left, Top (X/Y) at top right, Front (Y/Z) at bottom left and Side (X/Z) at bottom right. Drag the splitters to resize them (**View > Zoom and draw distance > Reset view splits** puts them back). Each view has a **View** menu in its corner to change what it shows (any quadrant can also show the 2D logical view). `Shift+Z` maximizes the view under the mouse, and `F2` to `F5` change its type. See [Camera and navigation](../features/camera-navigation.md) and [Render modes and display options](../features/render-modes-display.md).

## Separate panels

Right-click a side-panel tab and choose **Float panel**, or use the **Float panel** button beside the tabs. The panel opens in its own movable, resizable window inside the editor, so properties, textures, models and other panels can remain visible together. Repeat for as many panels as needed. **Dock panel**, or closing its floating window, puts it back in the side tabs. Floating choices and window placement survive restarts; visibility still follows **View > Panels**.

## Map tabs

Above the views, one tab per open map. A `*` after the name marks unsaved changes and the tooltip shows the full path. Click a tab to show it, middle click or the cross to close it, **+** for a new map. Right click: **Close**, **Close other maps**, **Copy the path**, **Show in the folder**. `Ctrl+Tab` and `Ctrl+Shift+Tab` cycle the tabs. See [Several open maps](../features/multiple-maps.md).

## Right dock

The panel on the right starts with the **selection bar** (as in Hammer's right bar): the selection mode (**Groups**, **Objects**, **Solids**) and **Move selected:** with **To world** and **To entity**. Below it, tabs:

| Tab | Content |
|---|---|
| Tool | Options of the active tool, when it uses the side panel. |
| **Textures** | The [texture browser](../features/texture-browser.md). |
| **Entity** | [Entity properties](../features/entity-properties.md) of the selection. |
| **Models** | The [model browser](../features/model-browser.md). |
| **Visgroups** | [Visgroups](../features/visgroups.md). |
| **Outliner** | The [outliner](../features/outliner.md). |
| **Prefabs**, **Instance**, **Lighting**, **Map assets**, **History**, **Sounds**, **Particles** | Panels of the feature packages: [prefabs](../features/prefabs.md), [instances](../features/instances.md), [lighting](../features/lighting-preview.md), [map assets](../features/map-assets.md), [undo history](../features/undo-history.md), [sounds](../features/sound-browser.md), [particles](../features/particles.md). |

Which tabs are present depends on **View > Panels**. By default the texture browser, entity properties, visgroups and console are shown; the outliner and model browser are not.

## Status bar

The bar at the bottom (hide it with **View > Panels > Status bar**) is made of segments.

| Segment | What it shows |
|---|---|
| Tool hint (left) | What the active tool expects, or the step of a running compile, or what a pending "pick in the views" request wants. |
| Selection | "No selection", the number of selected objects, or a description of a single object or group ("Solid n° 42, 6 faces", "Group n° 7 of 5 objects") with its size and center. Click to copy the id. |
| Cursor | The world position under the mouse. |
| Game content | Shown while the Workshop and downloaded addons still open ("Workshop N / M"), when no game content is set ("No game content", click to open the Game settings) or when content failed to load. |
| Map health | Problem counts of the live checks (click to open Check for problems), the instant **Leak** badge (click to go to the leak and show its path), and the gauge of the engine limit closest to its maximum (click to open Map information). |
| Compile | The step, progress and a Cancel button while compiling, the background compile, and a badge when the last compile leaked or failed. Click to open the compile window. |
| Game | "Game ready" while Garry's Mod runs and can take a map, "Game starting" while it boots. |
| Lighting | Bake progress with a Stop button, or the lightmap budget when enabled. Click to open the Lighting tab. |
| MCP | Shown while the MCP server of this window is on. |
| Right side | Frame rate (when **Show frame rate** is on), and the texture lock, snap and grid size when the toolbar, or that part of it, is hidden. |

## Console

The console panel at the bottom (`` ` `` or **Window > Console**) lists the messages of the editor: map loading, content mounting, compile results, errors of worker threads and of the graphics card. Filter buttons show or hide each kind (info, success, warning, error), a field filters by text, and buttons copy the visible messages or clear the console. The same warnings and errors are also written to the log file (see [Crashes and logs](../troubleshooting/crashes-and-logs.md)). It sits under the views only, between the tool palette and the right dock: it never covers the right dock, which keeps its full height down to the status bar. Drag its top edge to resize it; its height is limited so the views keep room.

## Notifications

Short messages (toasts) stack in the bottom right corner: "Saved", "Autosaved", results of operations, warnings. Click one to dismiss it.

## Loading card

While the game content mounts or a big map opens, a card appears in the middle of the views with the step, a progress bar with the count of archives and addons opened, and the one being opened. It only shows when loading takes more than a moment, and the menus, panels and the rest of the window stay usable. A map that is opening can be stopped with **Cancel**.

## Related pages

- [Your first map](first-map.md)
- [Settings window](../features/settings-dialog.md)
- [Themes and languages](../features/themes-languages.md)
- [Keyboard shortcuts](../features/keyboard-shortcuts.md)
- [Command palette](../features/command-palette.md)
- [Actions and shortcuts](../reference/actions-shortcuts.md)

---
[Back to the documentation index](../README.md)
