[Documentation](../README.md) > Getting started > Interface tour | [Français](../../fr/getting-started/interface-tour.md) | [Русский](../../ru/getting-started/interface-tour.md)

# Interface tour

The Hammer Pro window follows the layout of Valve Hammer and Hammer++ (menus in the same order, tools on the left, four views, properties on the right), so a mapper coming from them finds every button where they expect it. The interface is black and white (dark or light theme): color is kept for your map, textures and models.

## Window frame and title bar

Hammer Pro draws its own window frame: rounded corners, a soft shadow and a title bar that holds the Hammer Pro mark, the menus, the title `<map name> - Hammer Pro` and the minimize, maximize/restore and close buttons. Hovering the title shows the full path of the map ("Not saved yet" for a new map) and "Unsaved changes" when the map is modified.

If the frame misbehaves (remote desktop, some window managers), turn on **Settings > General > Native window frames**: the system's title bar comes back and the menus move to a menu bar under it. See [Window frames](../features/window-frames.md).

## Menu bar

| Menu | What is in it |
|---|---|
| **File** | New map, Open map..., Recent maps, Save, Save as..., Save as new version..., Close, Close other maps, import and export (Import map into current..., Export selection as VMF..., Export a copy..., Export again, Export OBJ..., BSP Decompiler..., BSP Sniffer..., Open archive..., Import a model (FBX, OBJ, glTF)..., Import props from the game), compile and run (Compile and run..., Compile now, Cancel compile, Batch compile..., Run the map in game, Reload the map in game, Build and save as content..., Publish on the Workshop...), Exit. |
| **Edit** | Undo, Redo, clipboard (Cut, Copy, Paste, Paste special..., Delete, Duplicate, Copy keyvalues, Paste keyvalues), selection (Select all, Clear selection, Invert selection, previous and next face selection), **Selection mode** (Groups, Objects, Solids, Ignore groups), Find entities..., Find and replace in entities..., Properties, Edit color..., Set maximum distance... |
| **Map** | Grid (Show grid, Snap to grid, Smaller grid, Larger grid, 3D grid on faces), **Grids** (global, per view, custom grids, grid manager), Map properties..., Map information, Check for problems, Entity report, Map assets, Check for leaks, Go to brush or entity id..., Soundscapes..., comparisons and suggestions, pointfiles, portal files, nav mesh, and **Lighting** (bake, cubemaps). |
| **View** | Hide selected, Hide unselected, Unhide all, cordons and visgroups, centering and zoom of the views, Maximize view, **View type** (Top, Side, Front, 3D, logical views), **3D render mode**, **Display options**, Player view, **Panels** (which panels and bars are shown), **Language**. |
| **Tools** | The tools (Selection, Camera, Entity, Block, Polygon, Curve, Road, Texture application, Decal, Overlay, Clipping, Extrude / carve, Bevel, Vertex, Displacement, Path, Cordon, Sprinkle), brush operations (Group, Ungroup, Carve, Make hollow, Merge, Tie to entity, Move to world...), **Align**, **Flip**, **Instancing**, Transform..., prefabs, placement and props, Find and replace textures..., Reload content from disk, the MCP server, Command palette..., Options... |
| **Window** | Console, **Layout** (Save layout..., Reset layout), Next map, Previous map. |
| **Help** | Keyboard shortcuts, Documentation, Entity help, Report an issue, Setup assistant..., About Hammer Pro. |

Every command, with its default shortcut, is listed in [Actions and shortcuts](../reference/actions-shortcuts.md). The [command palette](../features/command-palette.md) (`Ctrl+P`) finds any of them by name.

## Toolbar

The toolbar row under the menus follows Hammer's Map View and Map Operations toolbars, one group after another:

1. New map, Open map, Save.
2. Show grid, 3D grid on faces, the grid size (smaller, a field where you can type any size from 0.01 to 4096, larger), Snap to grid.
3. Undo, Redo.
4. Carve, Group, Ungroup, Ignore groups.
5. Hide selected, Hide unselected, Unhide all.
6. Cut, Copy, Paste.
7. Use cordons, Cordon tool, Radius culling.
8. Displacement mesh in 2D views, Displacement walkability, Show nodraw faces, Show model collisions, Texture lock, Texture scale lock.
9. **Compile**: opens the Compile and run window (Hammer's Run Map).
10. Show helpers and sprites, Show models, Show tool textures, Show editor objects, 3D skybox, Show particles, Simulate ropes.
11. The 3D render mode list, Player view, Check for problems.

The toolbar always stays on one row. When the window is too narrow for every group (a small screen, a large interface scale, the longer French or Russian labels), the least used groups go into the **»** button at the end of the row, in this order: the view toggles (10), the displacement and texture lock toggles (8), the cordon group (7), Cut, Copy, Paste (6), the hide and group buttons (5 and 4), then the render mode group (11). The groups that remain keep their order, and the **»** menu shows the hidden ones with the same buttons, tooltips and shortcuts; it stays open while you click its buttons. **Compile** is always the last to go.

At the right end of the top bar, the **MCP** pill shows whether the MCP server of this window is on, and its menu installs Hammer Pro in AI apps (see [MCP server](../reference/mcp-server.md)).

Under the toolbar, the **options row** shows the name of the active tool and its options. Tools with large options (Texture application, Displacement...) show them in the **Tool** tab of the right dock instead ("Tool options are in the right panel.").

## Tool palette

The vertical strip along the left edge holds the tools as icons, in groups separated by a line: moving around (Selection, Camera), creating (Entity, Block, Polygon, Curve, Sprinkle, Road), editing (Texture application, Apply current texture, Decal, Overlay, Clipping, Extrude / carve, Bevel, Vertex, Displacement), special tools (Path, Cordon, then Propper and Unpropper), and the isolate eye (isolate the selection, or show everything again). The tooltip of each button gives its name and shortcut. When the window is short the buttons get flatter, then the strip scrolls.

## The four views

The center of the window is a 2x2 grid of views: the 3D view at top left, Top (X/Y) at top right, Front (Y/Z) at bottom left and Side (X/Z) at bottom right. Drag the splitters to resize them (**View > Reset view splits** puts them back). Each view has a **View** menu in its corner to change what it shows (any quadrant can also show the 2D logical view). `Shift+Z` maximizes the view under the mouse, and `F2` to `F5` change its type. See [Camera and navigation](../features/camera-navigation.md) and [Render modes and display options](../features/render-modes-display.md).

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
| Right side | Frame rate (when **Show frame rate** is on), and the texture lock, snap and grid size when the toolbar is hidden. |

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
