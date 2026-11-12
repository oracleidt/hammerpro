[Documentation](../README.md) > Features > Leaks and pointfiles | [Français](../../fr/features/leaks-pointfiles.md) | [Русский](../../ru/features/leaks-pointfiles.md)

# Leaks and pointfiles

A leak is a way from an entity to the void outside the map: the compiler cannot tell inside from outside, so the visibility and the lighting fail. Hammer Pro finds leaks while you edit (the instant leak check), loads the pointfile of a compile, walks you along the leak path, and shows the portals and the visibility of the map so you can optimize it. It can also seal a doorway with an areaportal.

## The instant leak check

While you edit, a flood fill like the compiler's looks for leaks in the background, without compiling (setting **Look for leaks as I edit**, on by default). When it finds one, the status bar shows a red **Leak** badge and the path is drawn in the views (setting **Draw the path of the leak found**, on by default). The compile stays the reference.

**Map > Check for leaks** runs the check now, then:

- *The map leaks: entity N (class) reaches the void (M entities in all). Its path is drawn in the views.* The leaking entity is selected and the views are centered on it.
- *No leak: the map is sealed (checked in N ms)*.
- *No world brush seals the map yet: nothing to check*.
- *The map is too complex for the instant leak check: compile it to find leaks*.

Clicking the **Leak** badge of the status bar does the same.

## Pointfiles of a compile

When a compile finds a leak, the compiler writes a pointfile (`<map>.lin`). With **Load the leak pointfile automatically** (Settings > Compile, on by default) it is loaded at the end of the compile and the leaking entity is selected. The compile window can also stop right after the geometry on a leak (**Stop when the map leaks**, see [Compile window](../compiler/compile-window.md)).

| Command | What it does |
| --- | --- |
| **Map > Leaks and visibility > Load leak pointfile...** | Loads `<map>.lin`, else `<map>.pts`, next to the map or in its build folder (`.hammerpro_build`). Without one, asks for a file (filter **Leak pointfile**, `.lin` and `.pts`). |
| **Map > Leaks and visibility > Unload pointfile** | Removes the path from the views. |
| Drop a `.lin` or `.pts` file on the window | Loads it. |

A loaded pointfile replaces the path of the instant check in the views. When the leaking entity is deleted, the path is unloaded (*The leaking entity is gone: leak path unloaded. Compile again to check the map is sealed.*).

## The leak path navigator

A loaded pointfile opens a small floating window, **Leak path: N points**, to walk the camera along the path the way you fix a leak: point 1 is the leaking entity and **Next** walks toward the void.

| Control | What it does |
| --- | --- |
| **Point N of M** | Where you are on the path. |
| **Previous** | Previous point, toward the leaking entity. |
| **Next** | Next point, toward the void. |
| **Look at the current point** | Moves the 3D camera to look at the point. |
| **Unload** | Unloads the pointfile. |

The views label the ends of the path: **Leak start: entity** and **Leak end: void**.

## Portal files and the visibility

A portal file (`<map>.prt`) holds the portals between the visibility clusters that the geometry step builds. Viewing them shows where the visibility splits the map, and how hints and `func_detail` change it.

| Command | Default key | What it does |
| --- | --- | --- |
| **Map > Leaks and visibility > Load portal file...** | | Loads the `.prt` of the map (or asks for one). Dropping a `.prt` on the window works too. |
| **Map > Leaks and visibility > Unload portal file** | | Removes the portals. |
| **Map > Leaks and visibility > Build portals (geometry only)** | `Ctrl+Shift+P` | Builds only the geometry of the current map (seconds, into its build folder) and loads its portals: see the effect of hints and `func_detail` right away. |
| **Map > Leaks and visibility > Portals and visibility...** | | Opens the window below. |
| **View > Diagnostics > Show visible clusters** | | Colors the portals of the clusters visible from the camera (needs a map compiled with visibility). |

After a compile that wrote a new portal file, Hammer Pro offers it (*The compile wrote a new portal file.* **Load it** / **Not now**), unless you turned off **Offer the new portal file after a compile**. Portals already shown are reloaded automatically.

### The Portals and visibility window

| Control | What it does | Default |
| --- | --- | --- |
| Summary | *N portals between M clusters*, or **No portal file loaded**. | |
| **Line width** | Width of the portal lines, 1 to 4 px. | 1 px |
| **See through the walls (3D)** | The 3D view draws the portals over the geometry, like the 2D views. | on |
| **Color by render cost** | Portals go from green to red with the triangles drawn from their clusters: red areas are the ones to optimize (hints, func_detail, areaportals). Needs the visibility of the compiled map. | off |
| **Leaf grid (1024 units)** | The compiler splits leaves every 1024 units: hints work best along this grid. **Thickness** sets its line width (1 to 6 px). | off |
| **Show visible clusters** | Portals of the clusters visible from the camera (or a picked point) turn green, like `mat_leafvis 3` in game. **From the camera** or **From a point** (click a point in a view). Shows *Cluster N: X of Y clusters visible* and *Drawn from here: faces, triangles, displacements, props, overlays*. | off |
| **Geometry only and reload** | Same as **Build portals (geometry only)**. | |
| **Reload** | Reads the newest portal file of the map; after edits, it is built again first. | |
| **Load portal file...** / **Unload portal file** | As in the Map menu. | |
| **Detach window** | Shows the window as a window of its own (another screen). **Put back in the main window** returns it. Its place is remembered. | |

## Seal an opening with an areaportal

Select a door (`func_door`, `func_door_rotating`, `prop_door_rotating`, `func_movelinear`, or a brush of one) and run **Map > Leaks and visibility > Seal the opening with an areaportal**. Hammer Pro fits a `func_areaportal` brush to the doorway, links it to the door (the door gets the name `door_<id>` when it has none), makes it start open or closed like the door, and selects it. One undo step. Without a door selected you get *Select a door (func_door, prop_door_rotating...) to seal its doorway*.

The areaportal checks of [Check for problems](check-problems.md) tell you when an areaportal and its door are out of step, cross water or an occluder, or have more than one brush.

## Settings

**Settings > Map health** (`[health]`):

| Option | Key | Default |
| --- | --- | --- |
| **Look for leaks as I edit** | `live_leak_check` | on |
| **Draw the path of the leak found** | `show_leak_path` | on |
| **Portal line width** | `portal_width` | 1 |
| **Show the 1024 unit leaf grid** | `leaf_grid` | off |
| **Offer the new portal file after a compile** | `offer_portals` | on |
| Leaf grid thickness (portals window) | `leaf_width` | 1 |
| Color by render cost (portals window) | `portal_heat` | off |
| See through the walls (portals window) | `portal_xray` | on |

## Optimize the map

**Map > Optimize the map...** is an advisor that compiles the visibility of the map in the background and lists, sorted by impact, what is not optimized (visleaves, func_detail, hint brushes, areaportals, lights) with what to do and a **Go to** button. It never changes the map itself. See [Optimizing a map](../guides/optimizing-a-map.md).

## MCP

`check_leaks`, `get_leak`, `load_pointfile` and `portal_file` give AI apps the same tools. See the [MCP tool reference](../../MCP.md).

## Tips

- Fix a leak from the entity side: start at point 1 and press **Next** until you see the hole.
- After adding hints or `func_detail`, press `Ctrl+Shift+P` to see the new portals in seconds without a full compile.
- Turn on **Color by render cost** after a compile with visibility to find the expensive areas.

## FAQ

**The instant check says the map is sealed but the compile leaks. Why?** The compile stays the reference: some cases (entities in odd places, a very complex map) are only caught by the compiler. Load its pointfile.

**Where is the pointfile?** Next to the map or in the build folder `.hammerpro_build` beside it. **Load leak pointfile...** looks in both.

**Show visible clusters shows nothing.** It needs a map compiled with visibility (*No visibility data yet: compile the map with visibility*).

## Related pages

- [Check for problems](check-problems.md)
- [Compile window](../compiler/compile-window.md)
- [Common errors](../troubleshooting/common-errors.md)
- [Window frames and separate windows](window-frames.md)

---
[Back to the documentation index](../README.md)
