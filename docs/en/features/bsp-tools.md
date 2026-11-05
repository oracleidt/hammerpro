[Documentation](../README.md) > Features > BSP tools | [Français](../../fr/features/bsp-tools.md) | [Русский](../../ru/features/bsp-tools.md)

# BSP tools

Tools to work with compiled maps and archives: decompile a BSP back into an editable map, look inside a BSP pakfile, a GMA or a VPK (and edit a BSP's pakfile and entities), export the map geometry as OBJ for 3D tools, and compare two maps.

## BSP Decompiler

**File > BSP Decompiler...** asks for a `.bsp` and rebuilds an editable map from it, in the spirit of BSPSource. The work runs in the background (*Decompiling map...*) and the result opens in a new tab (*map decompiled in N ms (brushes: N, entities: N). Save it as a VMF to keep it.*).

What comes back:

- world brushes, with their texture axes and lightmap scales (material names patched by the compiler are turned back into the originals);
- brush entities with their brushes; detail brushes become `func_detail` entities, one per group of touching brushes;
- displacements, static props (`prop_static`), overlays (`info_overlay`, with the faces they lie on), cubemaps (`env_cubemap`), occluders and areaportal brushes;
- entities with their outputs, and ids from `hammerid`.

The materials and models of the map's pakfile are copied to a cache folder that is mounted while the map is open, so its own content displays (*Files of the map's own content, shown while it is open*).

A compiled map cannot give everything back: instances are merged, visgroups, groups and hidden objects are gone, and hint, skip and origin brushes were removed by the compiler. The console lists brushes, displacements and occluders that could not be rebuilt, overlays that lost their faces, and brushes without faces (a map protected against decompiling, see [Protect your BSP](bsp-protection.md)).

The BSP Decompiler works on a `.bsp` file you already have. To recover a map from a Garry's Mod server you join, see the [BSP Sniffer](bsp-sniffer.md).

## Open an archive (pack inspector)

**File > Open archive...** opens the **Pack inspector** window on a `.bsp`, `.gma` or `.vpk` (**Open...** in the window picks another one). Dropping one of these files on the Hammer Pro window opens it there too.

The header shows what the archive is: for a BSP its **Version**, **Revision**, number of **Entities**, **Pakfile** size and **Compressed lumps**; for a GMA its **Title**, **Type**, **Tags** and **Version**; for a VPK its **Version**.

| Control | What it does |
| --- | --- |
| **Filter entries...** | Filters the list of files. |
| **Extract...** | Extracts the selected entry, or every listed entry when none is selected, into a folder you pick. |
| **Add files...** (BSP) | Adds files to the pakfile. |
| **Replace...** (BSP) | Replaces the selected entry with a file. |
| **Remove** (BSP) | Removes the selected entry. |
| **Entity lump** (BSP) | Shows the entity lump as text you can edit (it must start with `worldspawn`). |
| **Save the BSP** (BSP) | Writes your changes after a confirmation (**Rewrite the BSP?**): a new file replaces the old one once complete. Keep a copy if you may need the old version. |
| **Discard** | Drops the pending changes (*N to add, N to remove*). |

Select an entry to preview it: text files (VMT, TXT, Lua, RES, CFG, JSON, VMF, RAD, FGD, VCD...), VTF textures (size, format, mipmaps, frames and the image), PNG, JPG, TGA and BMP images, MDL headers (version, textures, skins, bones), WAV files (channels, rate, bits) and PCF particle files (their systems). Entries the open map uses show in green (from the last analysis of the release window).

## Export OBJ

**File > Export OBJ...** writes the brush and displacement geometry of the map with texture coordinates to an `.obj` file and its `.mtl` material library next to it, for 3D tools (Blender, a model viewer...). Faces with `tools/` textures are left out. To round trip a selection with Blender and get it back in the map, use the [Blender link](blender.md) instead. Coordinates are turned from Source's Z up to the Y up most 3D tools expect. Texture sizes come from the game content when known (512 otherwise). Files are written atomically.

## Compare two maps

**Map > Compare with another map...** lists the differences between the open map and another VMF (an older version, an autosave, a copy). See [Check for problems](check-problems.md) for the **Compare maps** window.

## MCP

`decompile_bsp`, `inspect_archive` and `export_obj` give AI apps the same tools. See the [MCP tool reference](../../MCP.md).

## Tips

- Use the pack inspector on your release GMA to check what you ship before publishing.
- Decompiling is great to recover a lost VMF of your own map; save it as a VMF right away.
- Export OBJ to make a backdrop or a model in a 3D tool that matches your map exactly.

## FAQ

**Can I edit a GMA or VPK in the pack inspector?** No, only a BSP's pakfile and entity lump can be edited. GMA and VPK files are read only (extract and preview).

**Is the decompiled map identical to the original?** No. The geometry and entities come back, but instances, groups, visgroups, hidden objects and compiler-only brushes are lost.

**Where do the decompiled map's own materials come from?** From its pakfile, copied to a cache folder mounted while the map is open.

## Related pages

- [BSP Sniffer](bsp-sniffer.md)
- [Protect your BSP](bsp-protection.md)
- [Build and save as content](build-content-release.md)
- [VMF compatibility](../getting-started/vmf-compatibility.md)
- [Files and folders](../reference/files-and-folders.md)

---
[Back to the documentation index](../README.md)
