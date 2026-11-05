[Documentation](../README.md) > Features > Custom texture folders | [Français](../../fr/features/custom-texture-folders.md)

# Custom texture folders

A custom texture folder is any folder on your disk whose textures Hammer Pro shows directly in the texture browser: VMT and VTF files, or plain images (PNG, JPG, TGA, BMP) that become materials without any conversion. When you compile, the materials the map uses are converted and shipped with it.

## Adding a folder

- In the **Textures** tab, open **Custom folders (n)** and click **Add folder...**.
- Or **Settings > Game > Custom texture folders > Add folder...** (several folders can be picked at once).
- Or **New folder...** in the **Import images** window.

Adding a folder that is already in the list enables it again instead of adding it twice. The folders are saved in `custom_texture_dirs` of `[game]` in `settings.toml` (path, prefix, enabled).

## How material names are made

Hammer Pro reads the folder with these rules:

| Folder | Material names | Prefix field |
|---|---|---|
| It has a `materials` sub folder (a game style folder) | Taken from that `materials` folder, like a game folder; its `models`, `sound`... are mounted too. | Shown as `materials/`, not used. |
| It is itself inside a `.../materials/...` tree | The part of its path after `materials/` (`C:/art/materials/custom/bricks` gives `custom/bricks/...`). | Shown, not used. |
| Any other folder | `<prefix>/<path inside the folder>`. | Editable. |

For the last case the prefix is, in this order:

1. The prefix you typed.
2. When the field is empty (or holds the suggestion): the prefix the VMT files of the folder expect. A ready-made pack whose VMTs say `$basetexture "mypack/brick01"` next to `brick01.vtf` is mounted under `materials/mypack/`, so its materials work as in the game. The field then shows the prefix with a **detected** badge.
3. Otherwise the suggestion `custom/<folder name>` (`C:/Art/My Bricks` gives `custom/my_bricks`).

A typed prefix is cleaned: lowercase ASCII, forward slashes, accents folded, spaces and symbols turned into `_`, no `materials/` at the start. Press `Enter` (or leave the field) to apply it. Clear the field to go back to the automatic prefix.

## The folder list

Each folder of **Custom folders (n)** in the texture browser has:

| Control | What it does |
|---|---|
| Check box (**Use this folder**) | Enables or disables the folder. |
| Name | The folder name (hover for the full path). |
| Count | **N material(s)**, **Disabled**, or **not loaded yet** while the content reloads. |
| Filter button (**Show only this folder**) | Selects the folder's source chip so the grid shows only its materials. |
| Folder button (**Open in the file explorer**) | Opens the folder. |
| Bin button (**Remove this folder from the list (files are kept)**) | Removes the folder from the list. Nothing is deleted on disk. |
| **Prefix** | See above. |

Warnings appear under a folder when needed:

- **N material(s) use textures that cannot be found**: those VMTs reference textures found nowhere. A VMT saying `$basetexture "pack/brick"` needs its folder under `materials/pack/`: check the prefix. Hover the warning for the list.
- **N file(s) have names with spaces, accents or symbols**: rename them using lowercase letters, digits and underscores, the compile tools and the game may not find them. Images are renamed automatically.

Below the list:

| Button | What it does |
|---|---|
| **Add folder...** | Adds a folder of VMT/VTF files or plain images. |
| **Rescan** | Reloads the game content and the custom folders, to pick up new or changed files. |
| **Import images...** | Converts images into VTF and VMT files in a custom folder (see below). |

The same folders are listed in **Settings > Game > Custom texture folders**, with **Enabled**, **Path** and **Prefix** columns and a remove button.

## Plain images as materials

In every kind of folder, a PNG, JPG, JPEG, TGA or BMP image below the material root that has no VMT next to it becomes a material straight away:

- Hammer Pro writes a VMT for it in memory: shader `LightmappedGeneric`, `$basetexture` the image, plus `$alphatest 1` when its alpha is only fully opaque or fully transparent (fences), or `$translucent 1` when it has partial transparency (glass).
- Its name is made safe for the compile tools and the engine: `Mur Brique Été.png` becomes `mur_brique_ete`.
- The game needs sides that are powers of two: each side is rounded to the nearest power of two (ties round up, 4096 at most). The editor already maps the texture at that game size. The browser shows **Resized in game** on such materials, and the compile log warns about them.

## Import images

**Import images...** asks for image files, then opens the **Import images** window:

| Option | What it does | Default |
|---|---|---|
| List | The images to convert (**N image(s)**). | |
| **Destination** | The custom folder that receives the files. **New folder...** adds a custom folder. **No custom folder yet** when there is none. | first folder |
| **Surface property** | `$surfaceprop` written in the VMTs (**None**, or a common value such as `concrete`, `brick`, `metal`...). | **None** |
| **Generate mipmaps** | Writes mipmaps in the VTFs. | on |
| **Overwrite existing files** | Replaces VTF and VMT files that already exist (else those images fail). | off |
| **Import** | Starts (also `Enter`). The line above tells **Materials will be named prefix/<image name> in folder**. | |
| **Shader editor...** | Opens the [shader editor](shader-editor.md) on the first image of the list, to tint it, build a normal map or blend tiles before writing it. | |
| **Cancel** | Closes the window; during the conversion it stops after the current file. | |

The conversion runs in the background with a progress bar (**Importing N of M...**). Each image gives a `.vtf` (format chosen from its content) and a `.vmt` with the same alpha rule as above. Files go to `materials/<suggested prefix>` of a game style folder, or to the root of any other folder, under its prefix. The image name is cleaned the same way; two images with the same cleaned name: the later ones fail. The result is **N image(s) imported, M failed**.

## At compile time

The compiler always sees the custom materials. For the game:

- **Pack custom content** (Compile window option; **Pack custom content into the BSP** in the settings) packs the custom materials and models the map uses into the BSP; images are converted to VTF (DXT1 or DXT5 with mipmaps) and their VMT is written.
- Builds that do not pack copy the custom materials into `garrysmod/addons/hammerpro_dev` (**Development addon**), which the game loads.
- **Export custom materials** (**Export custom materials to the game** in the settings) also copies them into the game folder (off by default).

See [Compile window](../compiler/compile-window.md).

## Tips

- Keep one folder per texture pack and leave the prefix automatic: ready-made packs are detected from their VMTs.
- Name images in lowercase with underscores from the start; the material names will match the file names.
- Use power of two images (256, 512, 1024...) to avoid the resize warning.

## FAQ

**My pack's textures show as purple and black.** Their VMTs point to a path that does not match the prefix. Read the **use textures that cannot be found** warning and fix the prefix (or clear it so it is detected).

**I added files but they do not appear.** Click **Rescan**.

**Does removing a folder delete my files?** No, it only removes it from the list.

**Do players need my folder?** No: packing puts the used materials into the BSP.

## Related pages

- [Texture browser](texture-browser.md)
- [Material editor](material-editor.md)
- [Game setup](../getting-started/game-setup.md)
- [Compile window](../compiler/compile-window.md)
- [Map assets](map-assets.md)
- [Settings reference](../reference/settings.md)
- [Content index and scan](content-index.md)
- [MCP tool reference](../../MCP.md) (`import_images`, `list_asset_sources`)

---
[Back to the documentation index](../README.md)
