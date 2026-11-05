[Documentation](../README.md) > Features > Entity properties | [Français](../../fr/features/entity-properties.md) | [Русский](../../ru/features/entity-properties.md)

# Entity properties

The **Entity** tab of the right dock is Hammer's Object Properties: it edits the keyvalues, flags, outputs, model and visgroups of the selected entities, with a typed editor (SmartEdit) driven by the game data (FGD). Around it, Hammer Pro has tools that work on entity keys: eyedroppers, the color editor, copy and paste keyvalues, maximum distance, entity help, find entities, find and replace in entities, the entity report and the map properties.

## Opening it

- **Edit > Properties** (`Alt+Enter`) or **View > Panels > Entity properties** shows the **Entity** tab of the right dock.
- The tab always follows the selection:
  - one or more entities selected: their keys (several entities are edited together);
  - only world brushes selected: a "tie to entity" page (see below);
  - nothing selected: the worldspawn, that is the map properties.
- Every change is one undo step. Continuous changes (dragging a number, a color) merge into one step per key.

## The panel header

| Element | What it does |
|---|---|
| Kind icon | Point entity, brush entity, mixed selection or world. Its color is the class color of the FGD. |
| Class button | The class of the selection (or "N classes"). Click it to pick another class from a searchable list (point classes for point entities, brush classes for brush entities). Changing the class adds the default keys of the new class that are missing and applies to every selected entity. A brush entity that becomes a class with an `origin` key (`func_tracktrain`, `func_tanktrain`, `func_rotating`...) and has none gets it at the center of its brushes, like Hammer. |
| **?** | Opens the entity help of the class. |
| **☰** (**More**) | Menu with **Copy keyvalues**, **Paste keyvalues** (with the name of the copied class and its key count), the **Paste the name (targetname) too** check box, **Find and replace in entities...**, **Set maximum distance...** and **Entity help**. |
| **Entities selected: N** | Shown when several entities are edited. |
| **Move to world** | Shown for brush entities: gives the brushes back to the world and deletes the brush entities (`Ctrl+Shift+W`). |
| **Description** | Collapsed section with the FGD description of the class. |

Warnings appear under the header when no FGD is loaded (**No game data (FGD) loaded: properties are shown as plain text**) or when a class is not defined in the FGD (its keys are then shown as plain text).

For a `fog_volume`, the header says which fog it gets and what is wrong, in orange: **No Fog Name**, **No env_fog_controller named "x"**, **The fog of "x" is disabled (Fog Enable: No): in game this volume clears the fog**, **Preview: default fog look**, **Start Disabled**, and the state of **PostProcessName** and **Color Correction Name** (found, missing, or an entity of another class). For an `env_fog_controller`, it says when **Fog Enable** is No, whether the 3D view previews this fog and how many `fog_volume` use it. See [Fog volumes](render-modes-display.md#fog-volumes).

### Only world brushes selected

The panel shows **World brushes selected: N** and offers to give them a class: pick a brush entity class (by default `func_detail`, or the default brush entity of the settings) and click **Tie to entity** (`Ctrl+T`). The **Quick tie:** buttons create `func_detail`, `func_brush`, `func_illusionary`, `trigger_multiple` or `trigger_once` in one click (only the classes your FGD has). See [Brush operations](brush-operations.md).

## Tabs

| Tab | Content | Shown |
|---|---|---|
| **Properties** | SmartEdit: one typed widget per key. | Always |
| **Flags** | Spawnflags check boxes. | Entities |
| **Outputs (N)** | Outputs of the selected entities, with an editor. Highlighted when an output has a problem. | Entities |
| **Inputs (N)** | Outputs of other entities that target the selection. | Entities |
| **Model** | Skin, body groups and default animation of a studio model, with a preview. | When the common `model` key is a `.mdl` file |
| **Visgroups** | Visgroup membership of the selection. | Entities |
| **Raw** | Every key and value as a plain table. | Always |

The **Outputs** and **Inputs** tabs are described in [I/O and logical view](io-logical-view.md).

At the bottom of the entity tabs, the **Display color** section sets the color of the selection in the 2D views (see [Display colors](display-colors.md)), and the collapsed **Editor comments** section holds **Comments** (notes saved in the map). A selection of world brushes shows the **Display color** section under **Tie to entity**.

## Properties tab (SmartEdit)

Above the list:

- **Keys** toggles between the display names of the FGD and the raw key names.
- **Filter properties** keeps the rows whose key or display name contains the text.

Each row has a label and a widget. A `*` before the label marks a value that differs from the FGD default. Keys the FGD does not define are shown in italics. With several entities selected, a key that differs between them shows **(multiple values)**; the FGD keys shown are the ones common to every selected class.

Hover a label to see the display name, key, type, description, default value, and for distance keys a note on what the maximum distance does in game. Right click a label for:

- **Reset to default** (the FGD default, or removes the key when there is none);
- **Copy value**, **Copy key name**;
- **Remove key**;
- on the name key: **Renaming updates outputs** (on by default). When the name changes, the outputs of other entities that target the old name follow it, only when no other entity keeps that name.

Text fields apply on `Enter` or when they lose the focus; `Escape` cancels the edit. Double quotes are refused in values (the map file could not be read back).

### Widgets by key type

| Key type | Widget |
|---|---|
| Boolean | Check box (mixed state when the entities differ). |
| Choices | Drop-down list of the FGD choices. |
| Integer, float | Text field, red with a tooltip when the number is not valid (the value is then not applied). |
| Color (`color255`) | Color button, a **Brightness** field for light values (`r g b brightness`), and the text. A light value of `-1` (HDR not set) says that the regular value is used. |
| Color (`color1`) | Color button and text. |
| Studio model | Text, a browse button that opens the **Models** tab in "pick for this key" mode, and a **Recently used models** menu. A model missing from the mounted content gets a warning icon. See [Model browser](model-browser.md). |
| Material, decal, sprite | Thumbnail (bigger on hover), text, **Use the current material** button, and a browse button that opens the texture browser in pick mode (double click a material). Sprites get the `.vmt` extension. |
| Sound | Text, a picker listing the game sounds of the soundscripts then the sound files (with play buttons), a play/stop button, and a browse button to the sound browser. Only WAV, MP3 and OGG files can be played. See [Sound browser](sound-browser.md). |
| Particle system | Text, a picker of the particle systems of the content, and a browse button to the particle browser. See [Particles](particles.md). |
| Scene, script | Text and a picker of the `scenes/*.vcd` or `scripts/vscripts` files of the content. |
| Script list | Text and a **+** picker that appends a script to the list. |
| Face list (`sidelist`) | Text (the tooltip counts the faces) and a face eyedropper. |
| Name of this entity | Text with suggestions of the names of the map. |
| Target name, filter | Text with name completion, red when no entity has the name, an entity eyedropper and a **Mark** button. Filter keys also get a drop-down of the filters of the map. |
| Angles | Pitch, yaw and roll fields, a yaw dial (drag it, hold `Shift` for 15 degree steps) and an aim eyedropper. |
| Pitch (`light_spot`) | One number from -90 (straight down) to 90 (straight up), and an aim eyedropper. |
| Origin, vector | X, Y and Z fields. |
| Axis | Two points, X Y Z each. |
| NPC class, point class | Text and a class picker. |
| Instance file | Text and a browse button (a VMF file dialog; the path is stored relative to the map folder when possible). |
| `skyname` (worldspawn) | Text, a picker of the skies found in the content, and a hover preview of the six faces with **HDR faces found** or **No HDR faces**. |

For a single `func_instance`, its parameters are shown as typed fields under the list, with **Edit instance** and **Collapse selected instances**: see [Instances](instances.md).

Name completion: in target fields, `Tab` completes with the first name, the arrow keys pick one, `Enter` accepts; `Enter` alone keeps what you typed.

### Eyedroppers and Mark

| Button | Use |
|---|---|
| Entity eyedropper (target keys) | Click an entity in the views: its name is written to the key. If it has no name, a prompt suggests a free one (`class_1`, `class_2`...), names it and sets the key in one step. `Escape` cancels; clicking the button again also cancels. |
| Face eyedropper (`sidelist` keys) | Click faces in the 3D view (`Ctrl+click` for several, `Enter` to finish). `Ctrl+click` the button to add faces to the list instead of replacing it. |
| Aim eyedropper (angles) | Click a point in the views: each selected entity turns toward it (roll is kept). |
| **Mark** | Selects the entities the value names (names, `*` wildcards, class names) and centers the views on them. The tooltip gives their count. |

## Flags tab

One group per flags key (usually `spawnflags`): the raw value (sum of the checked bits, editable) and one check box per bit. Hover a box for its bit value and whether it is on by default. Bits unknown to the FGD show as **Unknown flag N**. With several entities, a bit set on only some of them shows a mixed box.

## Raw tab

A table of every key and value. `classname` is read only (use the class button). Edit a key name to rename it, edit a value to change it, **×** removes the key. The last row adds a key: type the name and value, then **+** or `Enter`. `id` and `classname` cannot be added from here.

## Model tab

Shown when the selected entities share a `.mdl` model.

- The name and path of the model, a preview (drag to orbit), the triangle and vertex counts and a **Statistics** section.
- **Display**: **Wireframe**, helper overlays (bounding box, movement box, hitboxes, bones, physics model, attachments, normals, origin axes, ground, illumination position, sequence box) and **Field of view**. Helpers the model lacks are disabled.
- **Skins**: sets the `skin` key.
- One drop-down per body group with at least two choices: sets the `body` key.
- **Sequences (N)**: the play button plays a sequence in the preview (**Stop the animation**, **Restart the animation**). Clicking a sequence name sets `DefaultAnim` when the class has that key. A newly shown model plays its `DefaultAnim` (else its first sequence).

## Visgroups tab

One check box per visgroup of the map, indented like the visgroup tree, with its color. A box is mixed when only some of the selected entities belong to the visgroup. Clicking adds or removes every selected entity. See [Visgroups](visgroups.md).

## Color editor

**Edit > Edit color...** (`C`) opens the **Color: key** window on the first color key of the first selected entity (the first `color255` key of its class, else `_light`, `rendercolor`, `_ambient` or `color`). Without one, a message says **The selection has no color key**.

- A color picker, **R G B** fields (0 to 255) and **H S V** fields.
- **Brightness** slider (0 to 2000, logarithmic) for light values.
- **Map palette**: 16 colors saved in the map (Hammer++ `palette_plus` block). Click a swatch to use it, right click to store the current color in it.

Changes apply live to every selected entity and merge into one undo step per key.

## Copy and paste keyvalues

- **Edit > Copy keyvalues** copies every key of the first selected entity except `id`, `classname`, `origin` and `targetname` (kept apart). The keys also go to the system clipboard as `"key" "value"` lines.
- **Edit > Paste keyvalues** writes them on every selected entity in one step. The class does not change.
- **Paste the name (targetname) too** (panel menu) or **Settings > Entities and browsers > Paste keyvalues also pastes the name (targetname)** also pastes the name. Off by default (`[entities] paste_targetname`).

## Set maximum distance

**Edit > Set maximum distance...** opens the **Maximum distance** window: one distance (2048 units at first, up to 1 000 000; the **0** button means no limit) written to the key each class honours in game.

| Classes | Key written |
|---|---|
| `light`, `light_spot` | `_distance`: the light stops there (models in game; Hammer Pro's compiler also fades the lightmaps over the last quarter). |
| `light_dynamic`, `point_spotlight` | `distance`, `SpotlightLength`: only shortened, never lengthened. |
| Props, items, weapons and other classes with fade keys, `info_overlay`, `env_detail_controller` | `fademaxdist`, with a `fademindist` start so they fade out instead of popping. For `env_detail_controller` it applies to every detail prop of the map. |
| `env_lightglow` | `OuterMaxDist`. |
| `func_lod` | `DisappearMaxDist` and `DisappearMinDist`. |

Logic entities, triggers and other classes the game cannot limit are skipped. The window shows how many entities will change, are already set and are skipped; **Apply** is enabled when something changes. The result is reported in a notification.

## Entity help

**Help > Entity help**, or **?** in the panel header, opens **Help: class** for the first selected entity (without a selection, the class of the entity tool). It shows the class kind and description, a **Valve Developer Community page** link, a filter, and sections **Keys (N)** (name, key, type, default, description), **Flags (N)**, **Inputs (N)** and **Outputs (N)**. `Escape` closes it.

## Find entities

**Edit > Find entities...** (`Ctrl+Shift+F`) opens **Find entities**.

| Option | What it does |
|---|---|
| Mode | **Name or class** (a text with `=` searches `key=value`), **Name**, **Class**, **Key=value**. |
| Search field | Text with `*` and `?` wildcards. Without wildcards the text can appear anywhere. |
| **Visible only** | Leaves hidden entities out. |
| **Exact match** | Off: the text can appear anywhere in the name. |
| **Select all results** | Selects every result. |

Results list class, name and origin. Double click a row (or `Enter` in the field) to select the entity and center the views on it. `Escape` closes the window.

## Find and replace in entities

**Edit > Find and replace in entities...** (`Ctrl+Alt+R`, `Ctrl+Shift+R` with the Hammer and Hammer++ key presets) opens **Find and replace in entities**. It searches key values, class names and outputs (output, target, input, parameter).

| Option | Default |
|---|---|
| **Find what**, **Replace with** | empty |
| **Whole words only** | off |
| **Match case** | off |
| **Key names too** (also find and rename key names) | off |
| **Whole map** / **Selection** | Whole map |
| **Visible only** | off |

The status line counts matches, fields and entities. **Find next** (or `Enter` in the find field) goes to the next match and selects its entity; **Replace** replaces in the current field and moves on; **Replace all** replaces every match in one undo step. Double click a result row to select its entity. Double quotes in the replacement are refused.

## Entity report

**Map > Entity report** lists every entity: **Class**, **Name**, **Details** (model, material, instance file or particle system), **Origin**, **Kind**, **Visible**. Click a column header to sort.

- Filter: words matched against class, name and details; a `key=value` word matches a key (`*` and `?` allowed). Every word must match.
- Kind filter (**All**, **Point**, **Brush**) and **Hide invisible**.
- Click, `Ctrl+click` and `Shift+click` select rows; **Select**, **Go to** (select and center the views) and **Delete** act on them. Double click a row to go to it.

Brush entities are placed at the center of their brushes. On maps with more than 3000 entities, the lists refresh a few times per second while you edit.

## Map properties

**Map > Map properties...** opens **Map properties**: the worldspawn with **Properties** and **Raw** tabs (the same editor as the panel, which also shows it when nothing is selected). Besides the FGD keys, it always shows the keys of Hammer Pro's lighting compiler:

| Key | Values |
|---|---|
| **Light file (lights.rad)** (`_lightsrad`) | A texture light file for this map. |

See [Worldspawn keys](../compiler/worldspawn-keys.md).

## Options

| Option | Where | Default |
|---|---|---|
| Paste keyvalues also pastes the name | **Settings > Entities and browsers**, `[entities] paste_targetname` | off |
| New outputs use the ESC separator | **Settings > Entities and browsers**, `[entities] io_esc_delimiter` | on |
| Sound preview volume | **Settings > Entities and browsers**, `[browser] sound_volume` | 80% |
| Renaming updates outputs | right click a name label | on |

## Tips

- When an FGD file changes on disk, Hammer Pro reloads the entity definitions within a few seconds (a log line says so).
- Use **Mark** on a target key to check at once which entities it reaches.
- Every feature here is reachable through MCP: `get_entity`, `set_entity_keyvalues`, `set_keyvalue`, `set_entity_class`, `set_spawnflags`, `copy_keyvalues`, `set_max_distance`, `find_entities`, `find_text`, `replace_text`, `entity_report`, `get_entity_class`, `get_map_properties`, `set_map_properties`, `set_visgroup_members`, and `run_action` for the windows. See the [MCP tool reference](../../MCP.md).

## FAQ

**Why are my keys shown as plain text?** No FGD is loaded, or the class is not in it. Check the game data in **Settings > Game** (see [Game setup](../getting-started/game-setup.md)).

**I selected a prop and a light: where are the other keys?** With several classes, the panel shows the keys common to every class. Select one class to see all its keys.

**Paste keyvalues did not change the class.** It never does: change the class with the class button, then paste.

**Set maximum distance skipped some entities.** Their classes have no distance the game honours (logic entities, triggers, plain brush entities, sprites, decals).

**How do I remove a key?** Right click its label and choose **Remove key**, or use **×** in the **Raw** tab.

## Related pages

- [Entity tool](entity-tool.md)
- [I/O and logical view](io-logical-view.md)
- [Model browser](model-browser.md)
- [Sound browser](sound-browser.md)
- [Texture browser](texture-browser.md)
- [Instances](instances.md)
- [Visgroups](visgroups.md)
- [Clipboard](clipboard.md)
- [Soundscapes](soundscapes.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
