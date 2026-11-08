[Documentation](../README.md) > Features > Color correction | [Français](../../fr/features/color-correction.md) | [Русский](../../ru/features/color-correction.md)

# Color correction

Hammer Pro creates the color lookup tables that Garry's Mod uses for `color_correction` entities, and previews those entities in the 3D views the way the game applies them.

Two tools:

- the **color correction editor**, which builds and edits a table with live before and after images and can show it in the 3D views;
- the **color_correction preview**, which applies the tables named by the `color_correction` and `color_correction_volume` entities of the map according to the distance of the camera.

## The file format and where it goes

A color correction table is a `.raw` file of exactly 98304 bytes: a 32 x 32 x 32 cube of RGB bytes (32 x 32 x 32 x 3), red varying fastest, then green, then blue. The identity table leaves every color unchanged.

A `color_correction` entity names its table in the **Lookup Filename** key (`filename`). The engine reads that path through its file system, relative to the game folder or an addon, so the key value must be the path of the file as it is shipped. The editor offers three folders:

| Folder | Notes |
|---|---|
| `scripts/colorcorrection` (default) | `scripts/colorcorrection/cc_name.raw`. |
| `materials/colorcorrection` | Same mechanism, another folder. |
| `materials/correction` | The Source SDK convention (`materials/correction/cc_name.raw`). |

Whatever folder you pick, the editor shows the exact `Lookup Filename` to type, with a **Copy** button. If you publish the addon on the Workshop, check that the folder you chose is allowed by the addon whitelist of the Garry's Mod addon packer.

## Opening the editor

- **Tools > Color correction editor...** (action `color_correction_editor`) opens the window, and again closes it.
- MCP: `open_media_editor` with `editor` set to `color` (and a `path` to load a `.raw`).

## The editor

The left column holds the controls, the right column the preview and the saving.

| Section | Parameters |
|---|---|
| **Presets** | Neutral, Black and white, Sepia, Warm, Cool, High contrast, Faded, Night, Vivid, Cinematic, Horror, Invert. A preset replaces the current settings. |
| **Basic** | Exposure (EV, in linear light), Brightness, Contrast, Gamma, Saturation, Vibrance (boosts the dull colors more), Hue (degrees), Invert. |
| **Color balance** | Temperature (cool to warm), Green / magenta, Shadows and Highlights (lift or lower only the dark or only the bright part). |
| **Tint and colorize** | Tint multiplies the colors by a color; Colorize replaces the hue by a color while keeping the brightness (sepia, duotone). Each has an amount. |
| **Levels** | Input black and white, midtones (gamma), output black and white, for the RGB master and for the red, green and blue channels. |
| **Curves** | A smooth curve per channel (RGB master, red, green, blue). Click to add a point, drag a point to move it, right click a point to remove it. |

Right click a slider to reset it. The order of the operations is: exposure, temperature and tint balance, brightness, contrast, shadows and highlights, levels, curves, gamma, hue, saturation and vibrance, tint, colorize, invert.

### Preview

**Before** and **After** show a built-in test pattern (sky, a hue bar, a gray ramp, skin tones, color swatches) or any image you load with **Use an image...**. The line under the images tells how far the table moves colors.

**Preview in the 3D views** starts automatically when the editor opens and updates as you change a control. Select one `color_correction` or `color_correction_volume` before opening the editor to preview the draft through that entity: its distance falloff, maximum weight, enabled state and volume limits still apply. Other map corrections keep their own tables. With no selected correction, the draft applies globally at full weight. Closing the window or unticking the option restores the map preview.

### Importing a .raw

**Import a .raw...** picks a file; the field next to it loads a game path (`scripts/colorcorrection/cc_name.raw`). The imported table becomes the base and every correction is applied on top of it, so you can refine an existing table. **Remove** goes back to the identity base.

### Saving

Pick the folder (root), the folder under it, and the name, then **Save the .raw**. The root is by default the folder above the `maps` folder of the saved map, otherwise the game folder. An existing file is replaced only if you tick **Replace the existing file**.

## Previewing the color_correction entities in the 3D views

The quickest way is the **3D Color Correction** render mode (**View > 3D render mode**, `Alt+9`, the mode list of the toolbar, MCP `set_render_mode` with `color_corrected`): the textured view with the lookup tables of the map entities applied, without turning on the setting below. It is the textured mode plus the preview, so it follows the same rules.

**View > Display options > Preview color_correction entities** (action `toggle_color_correction_preview`, setting `lab.color_correction_preview`, default off) applies the lookup tables of the map entities to the 3D views, from the editor camera or, in the [player view](player-view.md), from the player camera. It follows the game:

| Key value | Effect |
|---|---|
| `filename` | The table. An entity without a readable table does nothing (the MCP tool reports `no_file` or `file_not_found`). |
| `minfalloff`, `maxfalloff` | Full weight up to `minfalloff` units, fading to 0 at `maxfalloff`. When either is -1, or both are equal, the weight does not depend on the distance. |
| `maxweight` | The weight is multiplied by it. |
| `fadeInDuration`, `fadeOutDuration` | Seconds to fade in when the entity becomes active and out when it stops. |
| `exclusive` | While an exclusive entity has some weight, the non exclusive ones are ignored. |
| `StartDisabled` | The entity starts disabled (the inputs `Enable` and `Disable` only exist in the running game). |

Several entities blend: their weights are added (scaled down together when the total goes over 1) and the rest is the unchanged image. `color_correction_volume` entities (`filename`, `maxweight`, `fadeDuration`, `StartDisabled`) apply while the camera is inside their brushes.

The lookup is done by the 3D pass itself: the final image is converted to display colors, looked up in the table with trilinear filtering and converted back. The correction applies in every shading mode, and is separate from the map-wide Hammer++ color correction of the lighting settings, which still follows the baked lighting display.

## MCP

| Tool | Use |
|---|---|
| `color_correction_presets` | The presets, the grade parameters with their ranges, the file format and the path convention. |
| `color_correction_generate` | Builds a table from a `preset` or a `grade` (and an optional `base` table) and saves it as `root/folder/name.raw`; returns the exact `filename` key value. `dry_run` reports without writing. |
| `color_correction_inspect` | Checks a `.raw` (size, identity, average color, how far it moves colors). |
| `color_correction_preview` | The "Preview in the 3D views" of the editor: a preset, a grade or a `.raw`, until `off`. |
| `color_correction_zones` | Every color_correction entity with its file, distance, falloff, weight and status. |
| `open_media_editor` | Opens the window. |

The toggle is also available through `set_setting` (`lab.color_correction_preview`) and `run_action` (`toggle_color_correction_preview`).

## See also

- [Sound editor](sound-editor.md)
- [Lighting preview](lighting-preview.md)
- [Player view](player-view.md)
