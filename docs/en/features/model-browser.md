[Documentation](../README.md) > Features > Model browser | [Français](../../fr/features/model-browser.md) | [Русский](../../ru/features/model-browser.md)

# Model browser

The model browser lists every model (`.mdl`) of the mounted game content, grouped by folder, with a live 3D preview, the model's information, its body groups and sequences, an inspector that finds missing files and materials ("Crowbar lite"), and tools to extract, decompile and compile models. It is also where you choose the model of a prop.

## Opening it

- **View > Panels > Model browser** shows or hides the **Models** tab of the right dock. The tab is hidden by default (`show_model_browser = false` in the `[ui]` table of `settings.toml`).
- The browse button next to a model key in the entity panel opens the tab by itself (see "Choosing a model for a key" below).
- No default shortcut. With the Hammer++ keymap preset (**Settings > Keyboard**), the shortcut is `Shift+F10`.

While the game content is mounting, the tab shows a spinner. Without a game folder it says so and offers an **Options...** button that opens the Settings window (see [Game setup](../getting-started/game-setup.md)).

## Finding a model

| Control | What it does |
|---|---|
| **Search models** | Keeps the models whose path contains every word you type (any case). A word starting with `!` excludes the models that contain it: `vent office !dynamic`. |
| Source chips **All**, **Game**, **Addons**, **Workshop** | Keep only the models of that kind of source. A chip appears only when the mounted content has that kind of source. |
| **Garry's Mod content only** | Lists only what every Garry's Mod player has (no mounted games, addons or Workshop items). Shared with the sound and particle browsers and saved (`gmod_only` in `[browser]`, default off). |
| Folder dropdown | **All folders**, or one top level folder under `models/` (for example `props_c17`). |
| **Models: N** | Number of models listed. |
| Grid / list button (`⊞` / `☰`) | Switches between the thumbnail grid and the list. Saved (`model_grid_view` in `[browser]`, default: grid). |
| **+** | **File > Import > Import a model (FBX, OBJ, glTF)...**, see [Model import](model-import.md). |
| **++** | **File > Import > Import models (batch)...**: many files or folders at once, see [Batch import](model-import.md#batch-import). |
| **Recent (N)** | The last models you used (up to 8 shown, 16 remembered for the session), above the list. Click to select, double click to use. |

In the list, models are grouped under folder headers with their count; click a header to fold or unfold it. Each row shows the file name and, when the content has several sources, a badge telling where the model comes from: **GMod**, **game: cstrike**, **garrysmod folder**, **addon**, **Workshop 123456**, **custom** (a custom content folder). Hover a row for the full path.

In the grid, each tile is a render of the model (or, when it cannot be drawn, the thumbnail of its first material, or a `?`). The tile size is the **Tile size** slider of the [particle browser](particles.md) (48 to 256 points, default 96, `tile_size` in `[browser]`). Hover a tile for the path and the source.

Keyboard, while the pointer is over the list or grid: `Up` / `Down` move the selection, `Enter` does what a double click does.

## Using a model

- **Drag** a model from the list or grid into a view to place a prop there: a box shows where it lands (on the surface under the pointer in 3D). The prop is a `prop_static` when the model is a static prop, else a `prop_dynamic`. The skin chosen in the preview goes with it, and the model becomes the model of new props.
- **Double click** (or `Enter`) uses the model for new props, or writes it into the key you are picking for.
- The buttons under the model name:

| Button | What it does |
|---|---|
| **+ Use for new props** | The [entity tool](entity-tool.md) becomes active and places this model, as `prop_static` for a static prop model, else `prop_physics`, with the skin shown in the preview. |
| **Apply to selected props (N)** | Sets this model (and the preview skin, when it is not skin 0) on the selected entities that take a model (`prop_*` classes, and classes whose FGD gives them a studio model). One undo step. |
| **Apply to the property** | Only while picking for a key: writes the model into that key. |
| **Copy** | Copies the model path to the clipboard. |

## Choosing a model for a key

The browse button of a model key in the entity panel (see [Entity properties](entity-properties.md)) opens the Models tab with a banner: "Choose a model for *class (key)*, then double click it or press Apply." The model the key already has is selected. Double click a model, press `Enter`, or click **Apply** to write it (with the preview skin when the key is `model` and the skin is not 0); the panel goes back to the **Entity** tab. **Cancel** leaves this mode. Writing is one undo step.

## Preview

The preview shows the selected model in 3D:

- Drag to orbit, mouse wheel to zoom, double click to reset the view.
- **Skin N** dropdown: the skin shown (enabled when the model has several). It is reset to skin 0 when you select another model.
- `▦` toggles wireframe, `↺` resets the view.
- A newly selected model plays its first sequence, like the game.

The **Display** section has helpers drawn over the model (the options of the Half-Life Model Viewer):

| Option | Shows |
|---|---|
| **Wireframe** | The mesh as lines. |
| **Bounding box** | Box around the visible mesh. |
| **Movement box** | Movement hull of the model header (`$bbox`), the box Hammer uses to place entities like `info_player_start`. |
| **Hitboxes** | Hitboxes of the default set, in the bind pose. |
| **Sequence box** | Bounding box of the playing sequence (else the first one). |
| **Bones** | Skeleton in the bind pose, joints in orange. |
| **Attachments** | Attachment points (`$attachment`). |
| **Normals** | Vertex normals. |
| **Origin axes** | X in red, Y in green, Z in blue at the model origin. |
| **Illum. position** | The point the engine lights the model from (`$illumposition`). |
| **Ground** | Grid at the height of the model origin. |
| **Physics model** | Collision model of the `.phy` file. |
| **Field of view** | 10 to 120 degrees, default 40. |

These helpers and the camera are for the session; they are not saved.

## Information tabs

**Information**: size, box minimum and maximum, collision hull size (when the model has one), triangles and vertices (with the totals over every body group when they differ), meshes (draw calls), materials, bones, hitboxes, LODs, skins, body parts, sequences, static prop (yes or no), surface property, mass (when known) and MDL version. **Materials (N)** unfolds the material list.

When a selected prop uses another model, **Replace all uses (N)** gives the model of the browser to every entity of the map that uses the prop's model (one undo step).

**Body groups (N)**: one dropdown per body group that has a choice. It starts from the `body` value of the first selected prop using this model. **Value of the body key** shows the resulting number, and **Apply to selected props (N)** writes it into their `body` key.

**Sequences (N)**: the sequences with their frame count. Click one to play it in the preview; `▶` / `⏸` plays or pauses, `↺` restarts. Double click a sequence, or click **Set as default animation of selected props (N)**, to write it into their `DefaultAnim` key.

## Inspector ("Crowbar lite")

The **Inspector** tab reads the model header (no geometry) and checks the files around it:

- A status line: **Every file and material was found**, or **Missing files or materials: N**.
- Counts: bones, attachments, sequences, body parts, flex controllers, levels of detail (read from the VVD), and **Attachment names (N)**.
- The companion files (`.vvd`, `.vtx`, `.phy` when it exists); a missing one is in red. A model without collision has no `.phy` on purpose, which is not an error.
- **Included models (N)** (`$includemodel`), missing ones in red.
- One section per skin with its materials: a missing VMT in red ("Material (VMT) not found"), a VMT whose base texture is missing in orange ("Texture not found: ..."). Skins with problems are unfolded.

Tools under it:

| Button | What it does |
|---|---|
| **Shape preview** | Opens the [shape preview](real-shape-placement.md#shape-preview) window on this model: its real shape from the top, the front, the side and in 3D, with its dimensions, origin, lowest point and collision mesh. |
| **Extract files...** | Copies the model and its VVD, VTX, PHY and ANI files from the game content (VPK, GMA or folders) to a folder you choose, keeping their folders (`models/props/x.mdl`). |
| **Decompile with Crowbar** | Extracts the model to a temporary folder (`hammerpro_models` in the system temporary folder) and opens it in Crowbar. Crowbar.exe is asked the first time and remembered (`crowbar_exe` in `[entities]`); right click the button and pick **Choose another Crowbar.exe...** to change it. |
| **Compile a QC...** | Runs the model compiler on a QC file you choose, with `-game <game folder> -nop4 -verbose` and the extra arguments of **Settings > Compile**: the program set in **Settings > Compile > Model compiler** (NekoMDL for example), else the game's studiomdl (`bin/win64/studiomdl.exe`, else `bin/studiomdl.exe` of the Garry's Mod folder). Its output goes to the console; a **Cancel** button stops it. Greyed out when no model compiler is found. |

## Collision models in the views

Two toggles of **View > Display options** show the collisions of props:

| Option | What it does | Default |
|---|---|---|
| **Collision models** | Draws the collision hull of props over them in the 3D view, and in the model browser preview. Also in **Settings > Rendering**. `collision_models` in `[render]`. | Off |
| **Show model collisions** | The 3D views show each model as its collision model only (what the player bumps into); an orange box marks a model that has no collision model. Also a toolbar button. `collision_only` in `[render]`. | Off |

## Settings

| Key | What it does | Default |
|---|---|---|
| `[ui] show_model_browser` | The Models tab is in the dock. | `false` |
| `[browser] model_grid_view` | Grid instead of list. | `true` |
| `[browser] tile_size` | Grid tile size (48 to 256). | `96` |
| `[browser] gmod_only` | Garry's Mod content only (models, sounds, particles). | `false` |
| `[entities] crowbar_exe` | Crowbar chosen from the inspector. | none |
| `[render] collision_models` | Collision models over props. | `false` |
| `[render] collision_only` | Collision models instead of models. | `false` |

## Tips

- Pick the skin in the preview before dragging: the placed prop gets it.
- The **Inspector** tab is the fastest way to see why a model shows purple and black textures in game.
- Through MCP, an AI assistant can use `search_models`, `get_model_info` and `list_used_models` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**Why is the Models tab missing?** It is hidden by default: turn on **View > Panels > Model browser**.

**The grid shows a texture instead of the model, why?** The model could not be rendered (yet); the tile falls back to its first material, or `?` when it has none.

**Why is "Compile a QC..." greyed out?** studiomdl was not found in the `bin` folder of the configured Garry's Mod install, or the model compiler set in **Settings > Compile** does not exist.

**Dragging gives a `prop_dynamic`, double click a `prop_physics`: why?** A drop uses `prop_dynamic` for models that are not static props, while **Use for new props** uses `prop_physics`. Change the class in the entity panel if you need another one.

## Related pages

- [Entity tool](entity-tool.md)
- [Entity properties](entity-properties.md)
- [Model import](model-import.md)
- [Propper](propper.md)
- [Render modes and display](render-modes-display.md)
- [Map assets](map-assets.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
