[Documentation](../README.md) > Features > Camera and navigation | [Français](../../fr/features/camera-navigation.md) | [Русский](../../ru/features/camera-navigation.md)

# Camera and navigation

How to move around the map: the 3D fly camera, panning and zooming the 2D views, the three control schemes (Hammer Pro, Hammer, Hammer++), the Camera tool, view types and maximizing a view.

## 3D view: the fly camera

These controls work with every tool. They are those of the **Hammer Pro** and **Hammer** schemes; with **Hammer++ (middle mouse)** the middle button takes the place of the right one, which stays free for the tools (see [Control schemes](#control-schemes)):

| Input | Effect |
|---|---|
| Hold the right button and move the mouse (Hammer++: the middle button) | Look around |
| While that button is held: `W` `A` `S` `D` | Fly forward, left, back, right |
| `E` or `Space` / `Q` | Fly up / down (along world Z) |
| `Shift` | Fly faster (the fast speed multiplier) |
| Mouse wheel while flying | Changes the camera speed (x1.25 per notch, 50 to 20000 units/s); a toast shows the new speed |
| `Z` | Persistent mouselook: look around without holding a button. `Z` again or `Escape` leaves it |
| Middle drag (Hammer Pro and Hammer) | Pan (the scene under the pointer follows it) |
| Mouse wheel | Move forward or back, toward the cursor when **Zoom toward the cursor** is on. `Shift`: faster |
| Right click without moving | Opens the viewport context menu |

- **Smooth 3D camera** (on by default) gives the fly camera a short acceleration and deceleration: it ramps up to its speed in **Camera acceleration time** seconds and glides a moment after you release the key, and following a teammate in [Team Mapping](team-mapping.md) glides toward their camera. Turn it off for instant response: the camera starts at full speed the frame you press a movement key, stops the frame you release it, and jumps straight to a followed teammate. Toggle it in **Settings > Viewports**, in **View > Zoom, draw distance and camera > Smooth 3D camera**, in the menu of the 3D view header (click the view name), or with the action `toggle_camera_smoothing`. The acceleration time is kept while smoothing is off (0 also means immediate).
- Mouse look, the mouse wheel, panning, orbiting and [trackpad](trackpad.md) gestures are never smoothed: the camera follows the raw input in both modes. Hover picking and the Lit quality **Auto** see the camera as moving only while it really moves, so with smoothing off they return to full quality the frame the camera stops.
- `Ctrl` never flies: while flying or in mouselook, a left click selects the object at the crosshair (the center of the view), and `Ctrl+click` adds it to or removes it from the selection, like Hammer. The displacement brush keeps painting instead.
- Shortcuts do not fire while the keyboard flies the camera.

### Texturing while in mouselook

In mouselook (`Z`) you can texture the face under the crosshair without leaving the camera mode, with any tool:

| Input | Effect |
|---|---|
| `T` (`fly_apply_texture`) or right click | Apply the current material to the targeted face |
| `F` (`fly_apply_texture_brush`) or `Shift+right click` | Apply the current material to every face of the targeted brush |
| `V` (`fly_pick_texture`) or middle click | Pick the material of the targeted face as the current material |
| `N` (`fly_align_texture`) | Align the texture of the targeted face to the view (the camera right and up directions) |

- The targeted face (the first solid face along the view direction, up to 16384 units) is not highlighted: the crosshair and its label name it. A crosshair is drawn at the center of the view with the solid, side and material of the target, and a hint line at the bottom of the view lists the keys and the current material.
- Every edit is one undo step. The keys are actions, so they can be changed in **Settings > Keyboard** (`[keybindings]`) like any other; they only work in mouselook.
- The left click keeps selecting the object at the crosshair.
- Through MCP: `ui_key` with `z` enters mouselook, then `t`, `f`, `v` or `n` act on the face at the center of the view (`set_camera` aims, `list_keybindings` gives the keys).
- When the active tool uses the right click in the 3D view (the texture tool applies the current material), `Shift+right click` opens the context menu.

### Keyboard layout (camera keys)

**Camera keys** on the **Viewports** settings page decides which keys fly:

| Value | Fly keys | Mouselook key |
|---|---|---|
| **Auto (keyboard)** (default) | By key position: WASD on QWERTY, ZQSD on AZERTY (down is A on AZERTY) | The key at the QWERTY Z position (W on AZERTY); the key labelled Z also enters it |
| **AZERTY** | By label: Z Q S D, A down, E up | W |
| **QWERTY** | By label: W A S D, Q down, E up | Z |

## Control schemes

**Camera controls** on the **Viewports** settings page (`[workflow] nav_scheme`):

| Scheme | Adds |
|---|---|
| **Hammer Pro** (default) | The controls above |
| **Hammer** | In the 3D view: `Space` + left drag looks around, `Space` + right drag pans, left + right drag moves forward (pointer up) and strafes (sideways), the arrow keys look around (90° per second) when nothing is selected |
| **Hammer++ (middle mouse)** | Like Hammer++: the middle button looks around and flies with `W` `A` `S` `D` in the 3D view (a middle click without moving still reaches the tool, for example to open an instance). The right button never moves the camera: it stays free for the active tool, so the [Face edit tool](texture-tool.md) applies the current texture with a right click and paints it with a right drag, and a right click elsewhere opens the context menu. `Space` + left drag, `Space` + right drag, left + right drag and the arrow keys work as in the Hammer scheme |

What each mouse input does in the 3D view:

| Input in the 3D view | Hammer Pro | Hammer | Hammer++ (middle mouse) |
|---|---|---|---|
| Right drag | Look around, fly with `W` `A` `S` `D` | Look around, fly with `W` `A` `S` `D` | Nothing: free for the active tool (Face edit: paint the texture) |
| Right click | Context menu (Face edit: apply the texture) | Same | Same |
| Middle drag | Pan | Pan | Look around, fly with `W` `A` `S` `D` |
| `Space` + left drag | The active tool | Look around | Look around |
| `Space` + right drag | Look around | Pan | Pan |
| Left + right drag | The active tool and look around | Move forward and strafe | Move forward and strafe |
| `Z` | Mouselook | Mouselook | Mouselook |

The settings page lists the controls of the chosen scheme under the combo box, and the shortcut reference (`F1`) lists them under **Viewport controls**. Both lists are built from the bindings the views really use, so they always match the scheme.

## 2D views

| Input | Effect |
|---|---|
| Middle drag, or `Space` + left drag | Pan |
| Mouse wheel | Zoom (x1.2 per notch), toward the cursor when **Zoom toward the cursor** is on |
| `Ctrl+wheel` | Changes the grid size (default) or zooms every 2D view, see **Ctrl + mouse wheel in 2D** |
| `+` / `-` | **Zoom 2D views in** / **Zoom 2D views out** (every 2D view) |
| `1` to `9` | Zoom presets of the view under the mouse: `5` shows one unit per point (100%), each key below halves, each key above doubles |
| Arrow keys | Scroll the view under the mouse by an eighth of its size, when nothing is selected (or arrow nudging is off) |
| Right click without moving | Opens the context menu |

With **Zoom the 2D views together** on, zooming one 2D view zooms the others the same way. The header of each 2D view shows its zoom in percent.

### Scroll while dragging to the edge

With **Scroll the 2D view when dragging to its edge** on (page **Viewports**, off by default; setting `workflow.drag_autopan`), the 2D views scroll like in Hammer while you drag anything with the left button: moving or resizing a selection, the box of the Block tool, the pending box selection and its handles, vertex and clip handles, a box selection, an entity, a decal or overlay handle. Bring the cursor within 16 points of the edge of the view, or past it (even outside the window), and the view scrolls that way, faster the farther the cursor goes (from about 250 points per second at the edge up to 4000). What you drag stays under the cursor in world space and keeps snapping to the grid, in the Top, Front and Side views alike. The scrolling stops as soon as you release the button or bring the cursor back inside.

## Camera tool

`Shift+C` or **Tools > Active tool > Camera tool**. It edits the 3D camera from the 2D views, like Hammer's camera tool.

In a 2D view the camera is drawn as an eye marker with a line to its look target:

| Gesture | Effect |
|---|---|
| Drag the camera | Moves it; it keeps looking at its target. `Shift`: moves both |
| Drag the target | Moves the look target, the camera turns toward it |
| Drag on empty space | Places the camera at the press point, looking at the pointer |
| `Shift+drag` on empty space | Adds a new camera, placed the same way |
| `Page Up` / `Page Down` | Previous / next camera saved in the map |
| `Delete` | Removes the active camera (a default camera is made when the last one goes) |
| `Escape` | Cancels the drag |

With **Camera tool: click to zoom** (page **Selection**, off by default), a click in a 2D view zooms in twice (`Ctrl+click` zooms out), like Hammer's Magnify tool.

In the 3D view, a drag looks around and a click selects like the Selection tool (`Ctrl+click` adds or removes, clicking again selects the object behind).

Options bar of the tool:

| Control | What it does |
|---|---|
| **Camera {n}/{m}**, arrows, trash can | The cameras saved in the map (up to 256): previous, next, delete |
| **Position** | Camera position (X, Y, Z) |
| **Yaw**, **Pitch** | Camera angles (pitch from -89° to 89°) |
| **Look at selection** | Turns the camera toward the center of the selection without moving it |
| **Center 3D view on selection** | Moves the camera so the selection fills the view |
| **Reset** | Puts the camera back to its default position |

## View commands

| Command | Shortcut | What it does |
|---|---|---|
| **Center 2D views on selection** | `Ctrl+E` | Centers and zooms the 2D views on the selection |
| **Center 3D view on selection** | `Ctrl+Shift+E` | Points the 3D camera at the selection so it fills the view |
| **Maximize view** | `Shift+Z` | Maximizes the view under the mouse, or restores the four views |
| **Top view** | `F2` | Shows the Top view in the view under the mouse |
| **Side view** | `F3` | Side view |
| **Front view** | `F4` | Front view |
| **3D textured view** | `F5` | 3D view in Textured mode |
| **3D flat view** | `Shift+F5` | 3D view in Flat shaded mode |
| **Next 2D view** | `Tab` | Top, Front, Side in turn, in the 2D view under the mouse |
| **Shorter draw distance** / **Longer draw distance** | none | Divides or multiplies the 3D draw distance by 1.5 |
| **Reset view splits** | none | Puts the view splitters back in the middle |
| **3D view information** | `O` | Frame rate, camera position and angles, renderer statistics in the 3D view. A click copies `setpos` and `setang` for the game console |

They are in the **View** menu. The right click context menu of a view also has **Center views here**, and during a [Team Mapping](team-mapping.md) session **Ping here for the team**.

Each view has a header: click the view name to pick another view type (and, in 3D, the render mode and **Smooth 3D camera**), double click it to maximize the view, or use the maximize button at its right. Drag the splitters between the views to resize them, double click a splitter to reset it.

## Settings

**Tools > Options...**, page **Viewports**:

| Option | What it does | Default |
|---|---|---|
| **Camera controls** | Hammer Pro, Hammer or Hammer++ (middle mouse) | Hammer Pro |
| **Invert mouse X** | Inverts the horizontal axis when looking around | Off |
| **Smooth 3D camera** | Acceleration and deceleration of the fly camera, glide when following a teammate; off: instant starts and stops | On |
| **Camera acceleration time** | Time the fly camera takes to reach its speed (0 to 0.5 s) | 0.07 s |
| **Ctrl + mouse wheel in 2D** | Changes the grid size, or zooms every 2D view | Changes the grid size |
| **Zoom the 2D views together** | Zooming one 2D view zooms the others | Off |
| **Scroll the 2D view when dragging to its edge** | Dragging something to the edge of a 2D view scrolls it, like Hammer | Off |
| **Field of view** | 3D field of view (40° to 120°) | 75° |
| **Camera speed** | Fly speed (50 to 20000 units/s) | 1000 |
| **Fast speed multiplier** | Applied while `Shift` is held (1 to 20) | 3 |
| **Mouse sensitivity** | Look speed (0.02 to 1) | 0.15 |
| **Invert mouse Y** | Inverts the vertical axis | Off |
| **Camera keys** | Auto (keyboard), AZERTY or QWERTY | Auto |
| **View distance** | 3D draw distance (2048 to 262144) | 32768 |
| **Zoom toward the cursor** | Wheel zoom toward the cursor instead of the view center | On |
| **Outline objects under the mouse** | Outlines what is under the pointer in the 3D view | Off |

The keys are in `settings.toml` under `[view]` and `[workflow]`, see the [Settings reference](../reference/settings.md).

## Tips

- Lost in the map? Select something and press `Ctrl+Shift+E`, or use **Reset** in the Camera tool.
- Coming from Hammer or Hammer++, pick that scheme so `Space` and two button drags work as you expect.
- Spin the wheel while flying to set a speed that suits the size of the room.

## FAQ

**WASD does nothing.** The fly keys only work while the look button is held (the right button, the middle one with the Hammer++ scheme) or in mouselook (`Z`), with the pointer over the 3D view.

**With the Hammer++ scheme the right button also turned the camera.** Fixed in 2.0.8: the right button is now free for the tools like in Hammer++, the middle button looks around.

**My AZERTY keyboard flies the wrong way.** Use **Camera keys: Auto (keyboard)**, which works by key position (ZQSD), or force **AZERTY**.

**The camera moves too slowly in a big map.** Spin the wheel forward while flying, raise **Camera speed**, or hold `Shift`.

**Ctrl+wheel changes my grid instead of zooming.** Set **Ctrl + mouse wheel in 2D** to **Zooms every 2D view**.

## Related pages

- [Selection tool](selection-tool.md)
- [Interface tour](../getting-started/interface-tour.md)
- [Render modes and display options](render-modes-display.md)
- [Player view](player-view.md)
- [Grids](grids.md)
- [Keyboard shortcuts](keyboard-shortcuts.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
