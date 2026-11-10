[Documentation](../README.md) > Features > Blender link | [Français](../../fr/features/blender.md) | [Русский](../../ru/features/blender.md)

# Blender link

The Blender link is a round trip between Hammer Pro and Blender. Its main use is **displacements**: send one or several displacements (sewn or not) to Blender, sculpt and rework them with Blender's tools, and the result comes back into the **same displacements** with the new shape, keeping everything else the displacement holds. It also works for **brushes** (they come back as brushes) and for **props** (the model is recompiled and every prop that uses it is refreshed).

## The workflow in short

1. In Hammer Pro, select the displacements (the faces with the [displacement tool](displacement-tool.md), or the brushes that own them) and choose **Tools > Send to Blender**.
2. Blender starts with the displacements imported, one object per displacement, textured with the map's materials.
3. Sculpt or edit the vertices in Blender (sculpt mode, proportional editing, modifiers applied by hand...). Keep each object's vertices and faces: move them, do not add or delete.
4. Press **Update in Hammer Pro** (top bar **Hammer Pro** menu, or the sidebar tab), or just save the `.blend` file. Hammer Pro picks the file up within a second and rewrites the displacements. One undo step: **Update from Blender**.
5. Keep working: send again, or repeat 3 and 4 as often as you like.

## Opening it

- **Tools > Send to Blender** (greyed out while nothing is selected), and the matching button of the tool palette.
- **Settings > Blender link**: the program, the file format, the live link and the same actions as buttons.
- **Tools > Update from Blender** (greyed out without a link): reads the file now. **Tools > Stop the Blender live link** and **Tools > Install the Blender add-on**.
- The command palette has all four. No default shortcut.

## What is sent

| Selection | What is exported | What comes back |
|---|---|---|
| Displacements: selected displacement faces, or selected brushes and entities that have displacements | One mesh object per displacement, named `HPDISP_<solid>_<side>_p<power>` (the ids of the brush and of the face, and the power). Its grid of vertices in the exact order and triangulation of the displacement, with the final positions, normals and the UVs of the face. In glTF each object also carries the identity as custom properties (`hammerpro`: kind, solid, side, power, material). When a selection holds displacements, only they are sent. | The same displacements, reshaped. |
| Brushes, brush entities, groups (no displacement) | Every visible face with its texture coordinates and normals. `tools/` textures are left out. | New brushes in place of the sent ones. |
| A prop (`prop_*` with a `.mdl`) | The render mesh of its model, in the model's own coordinates. Every selected prop with the same model is linked to the file. | The model recompiled under `models/hammerpro`, then every linked prop uses it. |

Materials keep the name of the Source material (for example `dev/dev_measuregeneric01`) and their texture, as a PNG of 512 pixels at most (embedded in glTF, next to the file in OBJ and FBX), so the objects look right in Blender at once.

At most 1024 displacements per send.

### Formats

| Format | File | Notes |
|---|---|---|
| **glTF** (default) | `.glb`, one file | Textures embedded, identity in custom properties. The most reliable choice. |
| **OBJ** | `.obj`, `.mtl` and the PNG textures | One `o` object per displacement. |
| **FBX** | binary `.fbx` with the PNG textures next to it | Written as FBX 7.4, one model per displacement. |

The choice is saved (`blender.format`, default `gltf`). All three give the same result; Blender 5.2 was used to check the three round trips.

### Units and axes

Files are in meters with Y up, as Blender expects, with 1 meter equal to 39.37 Source units (the same unit as [model import](model-import.md)). Blender turns this into its own Z up (glTF) or keeps the file axes with a rotated object (OBJ, FBX): edit in the viewport as usual, the export converts back. Do not change the scene unit scale and do not apply or reset the object transforms of the displacement objects in a way that moves the vertices.

## What the update does to displacements

Each object of the file is looked at by its name:

- **Named `HPDISP_...` and its grid intact** (same number of triangles, vertices in place, texture coordinates unchanged): the new vertex positions are written into the displacement. Only the vertices you moved change (a move under about 0.02 unit is treated as no move). The alpha, the tags, the flags (collision), the lightmap scale, the power, the texture alignment, the blend rows (four way blend) and the allowed vertices stay as they are. A moved vertex keeps its direction vector: its height becomes the distance along it, and a sideways move is stored as the vertex offset. The triangle tags (walkable, buildable) are recomputed from the new shape.
- **Named `HPDISP_...` but remodeled** (you subdivided, deleted, merged or reordered): the new mesh is projected on the displacement, like [Conform to mesh](displacement-tool.md#conform-to-mesh), along the average normal of the displacement. The alpha, flags and tags are kept, the shape follows the mesh.
- **Any other object**, or a name that was not sent: it becomes new displacements with the **mesh to displacement** tiles (256 unit tiles, automatic power, the current material), at its place.

Then the seams between the updated displacements are **sewn** again, like **Sew** in the displacement tool. Displacements next to the updated ones that you did not send are not touched: send the neighbors too when a seam must follow.

Everything is one undo step. The console and the toast say how many displacements were updated, how many vertices moved, how many were reshaped to a new mesh, created or skipped (a displacement that was deleted or whose power changed since the send is skipped).

If you change the map after the send, the vertices you did not touch in Blender keep the map's current state: only what Blender moved is written.

## Brushes and props

- **Brushes**: the mesh is turned into brushes with the builder of [Convert prop to brushes](propper.md) (shape "render"): the sent brushes are deleted and the new ones take their place (in the same func_detail, or grouped in the world) and are selected. A mesh that cannot become brushes (more than 20000 triangles, or more than 1024 pieces) is compiled as a model and placed as a `prop_static` at the origin of the map, where it sits exactly over the old brushes; the link then follows that prop. After an undo, the next update replaces the brushes that exist again.
- **Props**: the model is recompiled (textures to VTF and VMT, collision, levels of detail, with the current [model import](model-import.md) options and the saved LOD and material parameter settings) under `models/hammerpro/<name>.mdl`. The name is the name of a model already in `models/hammerpro`, or the original name followed by `_blender`. The game content is mounted again, then every linked prop gets that model.

Material names: Blender adds `.001` to duplicated names, Hammer Pro removes that suffix. A material you create in Blender uses its name as the Source material name, so name it after a Source material to get the right texture on brushes.

## Starting Blender

Hammer Pro looks for Blender in this order: the setting `blender.path`, the Windows registry (the `.blend` file association and the Blender Foundation keys), the standard folders (`Program Files\Blender Foundation\Blender x.y`, the per user `Programs` folder, `/Applications`, `/opt`), a Steam library, then the `PATH`. The newest installed version wins. **Settings > Blender link** shows what was found.

It starts Blender with a small generated Python script (`startup.py` in the working folder) that empties the default scene, loads the add-on, imports the file and frames the view. Nothing is changed in your Blender settings. Sending again while the Blender that Hammer Pro started is still open does not start another one: the add-on there notices the new file within a second and replaces the scene with it.

When Blender is not found, the file is still exported and a window says so, with a **Browse...** button to pick `blender.exe` (or `blender-launcher.exe`), and **Start Blender** to retry. The program you choose is saved in `blender.path`.

## The Hammer Pro Bridge add-on

The add-on is a Python file Hammer Pro carries inside its program and writes on demand. It works with Blender 2.8 and later (checked with 5.2).

- **No installation needed**: it is loaded each time Hammer Pro starts Blender.
- **To have it in every Blender session**: **Tools > Install the Blender add-on** (or **Settings > Blender link > Install the Blender add-on**, or `blender_setup` with `install_addon`). It writes `hammerpro_bridge.py` into the `scripts/addons` folder of every installed Blender version found in your Blender configuration folder (`%APPDATA%\Blender Foundation\Blender\<version>\scripts\addons` on Windows, `~/.config/blender/<version>/scripts/addons` on Linux, `~/Library/Application Support/Blender/<version>/scripts/addons` on macOS). Then in Blender open **Edit > Preferences > Add-ons** and enable **Hammer Pro Bridge** (in Blender 4.2 and later it is listed under the legacy add-ons; use **Install from Disk** with the file if it is not listed).
- If no Blender configuration folder exists yet, the file is written to the working folder and the message tells you where, for **Install from Disk**.

In Blender the add-on gives:

- A **Hammer Pro** menu in the top bar and a **Hammer Pro** tab in the sidebar of the 3D view (key `N`), with the name of the linked file and **Update in Hammer Pro**, which exports the scene to the file Hammer Pro watches.
- **Export on save** (a checkbox, on by default): each time you save the `.blend`, the scene is exported the same way.
- A timer that loads a new file when Hammer Pro sends again.

The add-on finds the watched file through the environment variable `HAMMERPRO_LINK` (set when Hammer Pro starts Blender) or the link file `link.json` in the working folder. After **Stop the Blender live link** the buttons say there is no link.

## Live link

After a send, Hammer Pro watches the exported file (setting `blender.live_link`, on by default). When Blender writes it and the file stops changing for a short moment, Hammer Pro reads it in the background and applies it. With the live link off, use **Update from Blender** (or `blender_pull`) when you want the change. **Stop the Blender live link** ends the watching.

The watching costs nothing while the file does not change: a small background thread looks at the file's date and size four times a second and wakes the editor only when they change, so an idle Hammer Pro does not redraw while the link is on.

## MCP

| Tool | What it does |
|---|---|
| `send_to_blender` | Same as **Send to Blender**: `format`, `launch`, `live_link`. Answers with the file, what is linked (`displacements`, `brushes` or `prop`), and whether Blender started, was already open or was not found. |
| `blender_status` | Blender found or not, the settings, the working folder, the current link (file, format, kind, how many items, updates done), the last message. |
| `blender_pull` | **Update from Blender** now (one undo step); answers with what was updated. |
| `blender_setup` | `path` (checked: a Blender program or folder), `format`, `live_link`, `detect`, `install_addon`, `stop_link`. |

The settings `blender.format` and `blender.live_link` also work with `set_setting`; `blender.path` is changed with `blender_setup`, which checks that the file is a Blender program. See the [MCP tool reference](../../MCP.md).

In automated runs (`--script`, `--screenshot`) Blender is never started and the working folder is a temporary one.

## Settings

All in **Settings > Blender link** and in the `[blender]` table of `settings.toml`.

| Key | Default | What it does |
| --- | --- | --- |
| `blender.path` | `""` | The Blender program (`blender.exe`, `blender-launcher.exe`, or the Blender folder); empty means Hammer Pro searches (see "Starting Blender"). **Browse...** picks it, **Detect** clears it. |
| `blender.format` | `"gltf"` | File format of a send: `gltf` (a `.glb`), `obj` or `fbx`. |
| `blender.live_link` | `true` | Watch the exported file after a send and update the map when Blender writes it. |

Files used: the working folder is `HammerPro/blender` in the local application data folder of your account (set the environment variable `HAMMERPRO_BLENDER_DIR` to move it); it holds the exported file, the textures, `link.json`, `startup.py` and `hammerpro_bridge.py`. Old exports there are replaced each time you send.

## FAQ

**Nothing happens when I save in Blender.** Check that the link is still active (Settings > Blender link shows the linked file), that **Export on save** is on in the Hammer Pro tab, or press **Update in Hammer Pro**. The file must be the one in the working folder.

**The update says a displacement was reshaped to the new mesh.** Its vertices or faces were added, deleted or reordered, so the exact grid could not be read. The shape is projected instead; keep the grid (move vertices, do not change the topology) for an exact result.

**A displacement did not change.** It was deleted since the send, its power changed (for example you subdivided it), or the move was under the tolerance. Send it again.

**The seams opened.** Only the displacements you sent are sewn. Send the neighbors too, or use **Sew** afterwards.

**My prop did not change in the 3D view.** The game content is mounted again after the recompile, which takes a moment on big setups. The console says "Blender update: ... rebuilt".

**Textures look wrong on my brushes after an update.** Materials with the same name as the Source material keep it. Rename them in Blender to the Source material path.

## Related pages

- [Displacement tool](displacement-tool.md)
- [Model import](model-import.md)
- [Propper](propper.md)
- [BSP tools: Export OBJ](bsp-tools.md#export-obj)
- [MCP server](../reference/mcp-server.md)

---
[Back to the documentation index](../README.md)
