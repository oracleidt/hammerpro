[Documentation](../README.md) > Guides > Collect content for a release | [Français](../../fr/guides/collect-content-for-release.md) | [Русский](../../ru/guides/collect-content-for-release.md)

# Collect content for a release

A map that uses custom textures, models or sounds needs them in game, or players see purple checkerboards and errors. This guide checks what the map uses, then ships it: either a complete release with **Build and save as content**, or a plain copy of the files with the **Map assets** tab.

## Step 1: see what the map uses

1. Open **Map > Map assets**. The **Map assets** tab of the right dock lists every file the map uses: materials with their textures, models with their files and materials, sounds with their scripts, particle files. Each entry says where it comes from (base game, addon, Workshop, custom folder, loose file, mounted game, generated) and whether it is missing.
2. Use the filters: **All**, **Custom only** (everything that is not part of the base game, plus missing files) and **Missing**.
3. Fix every **Missing** entry: correct the material or model name in the map, or add the content (a custom texture folder, an addon), then click **Refresh**.

Base game files are never needed: every player has them.

## Step 2 (recommended): Build and save as content

For a Garry's Mod release, **File > Publish > Build and save as content...** does everything: Final compile, cubemaps and navigation mesh, packing, addon folder, GMA and Workshop upload.

1. In the **Build** tab, choose what to **Produce**:
   - **Workshop addon**: the BSP, a content folder and a `.gma`.
   - **Self-contained BSP**: one BSP with every needed file inside its pakfile.
   - **Server kit**: a self-contained BSP plus a FastDL folder and a Lua file adding the map to the downloads.
2. Check the **Output folder** (default: `<map>_content` next to the map; **Change...** to pick another).
3. Choose the options: **Final compile first**, **Build cubemaps and navigation in game** (starts Garry's Mod after a confirmation), **Ship content from Workshop items** (off: they are listed as Required Items), **Ship content from mounted games**, **Write the .gma**, **Compress the pakfile (LZMA)**.
4. Click **Analyze** to list every file the map needs without compiling or writing anything, and read the **Report** tab.
5. Fill in the **Addon** tab: **Title**, **Type**, **Tags (2 at most)**, **Description**, **Ignore** (gmad wildcards), and the thumbnail (**Capture the 3D view** or **Choose an image...**).
6. Open the **Checklist** tab: every row should be green before publishing. Most rows have a button that fixes them.
7. Click **Build**. The next builds are incremental: unchanged files are skipped (see `.hammerpro_release.toml` in [Files and folders](../reference/files-and-folders.md)).
8. To upload, use the **Publish** tab (**Publish as a new item...** or **Update the item...** with a **Change note**). Steam must be running.

Every option is described in [Build and save as content](../features/build-content-release.md).

## Step 2 (alternative): collect the files into a folder

To get a plain copy of the files (to make your own addon, send the content to someone, or archive it), use the **Collect** section of the **Map assets** tab:

1. Check the entries to collect (**Check all**, **Check none**, or one by one: an entry brings everything it needs).
2. Choose the **Output** folder (**Choose a folder...**). It must not be mounted game content.
3. Options:
   - **Also copy base game assets**: off by default, every player has them.
   - **Replace existing files**: off, a file the folder already has with other content is kept (and listed in the report); on, it is replaced after a confirmation.
4. Click **Collect N files**. The files are copied under `materials/`, `models/`, `sound/`... (files inside VPKs and GMAs are extracted), and a report `<map>_assets.txt` is written in the folder. **Show the report** and **Open the folder** open them.

## Clean up an addon: unused files

The section **Unused files of a content folder** lists the files under `materials`, `models`, `sound` and `particles` of a folder (an addon...) that this map does not use. Choose the folder and click **Find unused files**. It is a report only: nothing is deleted.

## FAQ

**Some files come from mounted games (CS:S, HL2 episodes).** Players without these games see errors. Ship them (**Ship content from mounted games**, at your own responsibility) or replace them with base game content.

**A soundscript is "not loadable".** Soundscript files cannot ship with a map: move the entries into `maps/<map>_level_sounds.txt`.

**Do I need to collect the models made by Propper?** No extra step: they are found like any other model of the map and shipped by the build.

## Related pages

- [Map assets](../features/map-assets.md)
- [Build and save as content](../features/build-content-release.md)
- [Custom texture folders](../features/custom-texture-folders.md)
- [Compile and test in Garry's Mod](compile-and-test-gmod.md)
- [Files and folders](../reference/files-and-folders.md)

---
[Back to the documentation index](../README.md)
