[Documentation](../README.md) > Features > Model import | [Français](../../fr/features/model-import.md) | [Русский](../../ru/features/model-import.md)

# Model import

Model import turns a 3D model file (FBX, OBJ, glTF or GLB) into a Garry's Mod prop without Crowbar or Valve's compilers: Hammer Pro converts its textures to VTF and VMT, compiles the model with its built-in compiler, then makes it the model of new props and can place one in front of the camera.

## Opening it

- **File > Import > Import a model (FBX, OBJ, glTF)...** opens a file dialog (filter "3D models": `.fbx`, `.obj`, `.gltf`, `.glb`).
- The **+** button of the [model browser](model-browser.md) does the same. Its **++** button opens the [batch import](#batch-import).
- **File > Import > Import models (batch)...** converts many files at once.
- Drag and drop a `.fbx`, `.obj`, `.gltf` or `.glb` file onto the Hammer Pro window. One model at a time: a second drop while an import runs is refused with a message. Dropping several model files at once opens the [batch import](#batch-import) with them instead.
- `--script` files have `import_model PATH`, `import_model_now PATH` (imports with the default options) and `import_model_scaled SCALE PATH` (see [Script commands](../reference/script-commands.md)).

The file is read in the background ("Reading ..."), then the **Import a model** window shows its options.

## Supported files

| Format | What is read |
|---|---|
| Wavefront OBJ (`.obj` with its `.mtl`) | Meshes and materials. Faces without normals get smooth normals per smoothing group (`s off` is flat), or across edges that are not too sharp when the file has no smoothing group. |
| glTF 2.0 (`.gltf`, `.glb`) | The meshes of the default scene with their node transforms (skinned meshes in their bind pose); the materials (base color texture with its coordinate set and `KHR_texture_transform`, color factor, alpha mode, double sided, and the normal, metal and rough, emissive values of "Material parameters"). Draco or meshopt compressed meshes are refused with a message. |
| FBX 7.x, binary and ASCII | Meshes placed by their models (translation, rotation and its order, pivots, offsets, scale, geometric transform, parents), materials (color, diffuse texture as a file or embedded, normal, bump and emissive textures, specular and emissive values), the axis system and the unit of the file. |

Only static geometry is imported: animations are ignored. Textures can be PNG, JPG, TGA or BMP.

## Options

The window first shows the file path and a summary: triangles, materials and size in units. Warnings appear for more than 65000 triangles (a heavy prop), and for a model smaller than 2 units or larger than 4096 units (check the scale).

| Option | What it does | Default |
|---|---|---|
| **Name** | `models/hammerpro/<name>.mdl`. Cleaned to lowercase letters, digits and `_` (48 characters at most). A warning says when a model of that name exists: it is replaced. | The file name |
| **Kind** | **prop_static**, or **Physics prop** (`prop_physics`). | prop_static |
| **Scale** | Extra scale after the unit conversion (0.001 to 1000). | 1 |
| **Convert the units** | Meters of the file become Source units (inches): 1 m = 39.37 units. Off: one unit of the file is one Source unit. | On |
| **Up axis** | **From the file**, **Y** or **Z**. Y up models are turned so that Z is up and the front stays in front. | From the file |
| **Origin** - **At the bottom center** | Puts the model origin at the center of its bottom. | On |
| **Surface property** | Physics and sound material: `default`, `concrete`, `metal`, `solidmetal`, `metal_box`, `metalpanel`, `wood`, `wood_crate`, `wood_furniture`, `plastic`, `plastic_barrel`, `glass`, `rock`, `dirt`, `grass`, `gravel`, `brick`, `tile`, `carpet`, `cardboard`, `paper`, `rubber`, `flesh`, `ice`, `porcelain`, `computer`, `pottery`. | default |
| **Collision** | **Pieces**: each separate piece of the model gets a convex hull. **Box**: the box of the model. **None**: no collision model. | Pieces |
| **Mass** (physics prop only) | 0.1 to 50000 kg. | 20 kg |
| **Levels of detail (LOD)** | Simplified versions of the model that the game draws far away. Shared with [Propper](propper.md) and saved (`[lab.model_lod]`). | On |
| **Levels** | Levels below the full model, 3 to 6 (big models get one or two more). | 4 |
| **Each keeps** | Part of the triangles of the level above that each level keeps, 20 to 90 % (small models are reduced more gently). | 50 % |
| **Keep material parameters** | Writes more than the base color into the VMTs: normal map, shine, self illumination, texture tiling and offset (see "Material parameters" below). Off: color, alpha and double sided only, and the tiling is baked into the UVs. Saved with the settings (`lab.model_keep_params`, also reachable with the MCP `set_setting`). | On |
| **Place it in front of the camera** | After the import, places a prop of the model where the 3D camera looks. | On |

A physics prop without collision gets a warning: it cannot move in game.

**Materials** lists the materials the model uses with their texture: the file name, "texture in the file" (embedded), or "no texture (its color)".

The options other than the name and the LOD settings stay as you left them until you close Hammer Pro. **Import** is greyed out while the name is empty.

## What is written

The model goes into the game folder of the mounted content, its textures next to it:

- `materials/models/hammerpro/<name>/`: one VTF (with mipmaps) and one `VertexLitGeneric` VMT per used material. A material without a texture (or whose texture cannot be read) gets a small texture of its color, with a warning in the console. Double sided materials get `$nocull`; alpha becomes `$alphatest` or `$translucent` (from the material's alpha mode, or from the texture's alpha when the file does not say); a tinted textured material gets `$color2`, a see-through color `$alpha`.
- With **Keep material parameters**: the extra files and keys described in "Material parameters" below.
- `models/hammerpro/<name>.mdl` with its `.vvd`, `.dx90.vtx` and `.phy` (no `.phy` with collision **None**). Older `.dx80.vtx`, `.sw.vtx`, `.vtx` and `.ani` files of the same name are removed. The MDL is written last, so the game never sees half a model.

The built-in compiler makes what studiomdl makes of a static prop: one bone, one idle sequence, one mesh per material, levels of detail, and a shadow LOD for physics props. A model with more than 65536 vertices is split into several body parts (the console says how many). The collision hulls are shrunk by the collision margin like studiomdl's; flat pieces are thickened to one unit.

**Build and save as content** packs these files with the map (see [Build and save as content](build-content-release.md)).

## Material parameters

With **Keep material parameters** (on by default) each material also gets what its file says beyond the base color:

| Source in the file | Written in the VMT |
|---|---|
| Normal map: glTF `normalTexture` (green channel flipped to the Source convention), OBJ `norm`, FBX `NormalMap` | `$bumpmap` with a `<material>_n.vtf` normal map. OBJ `map_Bump` or `bump` and FBX `Bump` are normal maps when the picture is bluish and height maps otherwise (converted to a normal map). |
| glTF `metallicFactor`, `roughnessFactor` and `metallicRoughnessTexture` (its mean, times the factors), `KHR_materials_pbrSpecularGlossiness`; OBJ `Pr`, `Pm`; OBJ `Ks` with `Ns`; FBX `SpecularColor` with `ShininessExponent` | `$phong 1` with `$phongexponent` (from the gloss), `$phongboost`, `$phongfresnelranges`, and `$phongalbedotint 1` for metals. A flat normal map is added when the file has none, because phong needs a `$bumpmap`. A dull material (roughness above 0.8) and a weak specular (OBJ `Ks` below 0.55 or `Ns` below 50, which skips the default of most exporters) get no phong. |
| A smooth metal (metallic 0.6 or more and roughness below 0.6) | `$envmap env_cubemap` with `$envmaptint` from metallic times gloss. |
| glTF `emissiveFactor`, `emissiveTexture` and `KHR_materials_emissive_strength`; OBJ `Ke`, `map_Ke`; FBX `EmissiveColor` | `$selfillum 1` with `$selfillummask` (`<material>_illum.vtf`, the emissive texture or a white one) and `$selfillumtint` for the color. |
| glTF `KHR_texture_transform` (scale and offset), OBJ `map_Kd -s -o`, FBX `Scaling` and `Translation` of the texture | `$basetexturetransform`, and `$bumptransform` with a normal map. A glTF rotation stays baked in the UVs. |

The console lists, for every material, what was converted and what was ignored (an occlusion texture, a specular or shininess texture, a height map, a texture file that cannot be found, glTF extensions such as `KHR_materials_clearcoat`, which are named). Textures are never mixed or repacked: only the maps above are read.

## After the import

- The content is mounted again and a toast says "*model* is ready (N textures, N collision pieces): it is the model of new props (Entity tool)". The console lists the levels of detail with their triangle counts.
- The model becomes the model of new props, with `prop_static` or `prop_physics` as class, for the [entity tool](entity-tool.md).
- With **Place it in front of the camera**, a prop is created where the 3D camera looks: resting on the surface hit (its origin is at its bottom center), else in the air in front of the camera. It becomes the selection (one undo step).

## Batch import

**File > Import > Import models (batch)...** (also the **++** button of the [model browser](model-browser.md), and `import_models_batch` in the command palette) converts many files in one run, with the same pipeline and the same options as a single import.

1. **Add files...** picks several model files, **Add a folder...** picks folders. **Include subfolders** walks them (24 levels at most), and the **Formats** boxes (OBJ, FBX, glTF / GLB) filter what is taken from folders. Files picked by hand are kept whatever their format box says as long as the extension is supported. At most 5000 files are taken, in alphabetical order, duplicates removed.
2. The **preview table** reads every file in the background and lists its triangles, its size in units, the model name it will get and its warnings (heavy model, too small or too large, a physics prop without collision, an unreadable file in red). A name in orange means a model of that name exists already. **Preview** reads the files again after you change the scale, units or axis.
3. **Name template** builds each model name: `{name}` is the cleaned file name, `{folder}` the name of its folder, `{index}` its number (001, 002...). The **Prefix** goes in front. The result is cleaned like a single import name (lowercase letters, digits and `_`, 48 characters at most); two files that give the same name get `_2`, `_3`... so none overwrites another.
4. The options are the ones of the single import: kind (`prop_static` or physics prop with its mass), scale, **Convert the units**, up axis, origin at the bottom center, surface property, collision, plus the saved **Levels of detail** and **Keep material parameters** settings.
5. **Replace models that exist** is off by default: a model whose name exists in `models/hammerpro` is skipped and reported. **Parallel conversions** sets how many files are converted at once (1 to 16; the default is half the processor threads, between 1 and 4, because textures and meshes take memory).
6. **Place one prop per model in a grid** puts one prop (`prop_static` or `prop_physics`) per converted model in front of the 3D camera: the grid starts where the camera looks, has the chosen number of **Columns**, and the **Gap** between cells (each cell is as big as the biggest model). The props rest on the floor level of that point and are selected, as one undo step ("Import models").
7. **Convert N models** starts the run. A progress bar shows the files done; **Cancel** stops after the files being converted (the rest is reported as not run). The **Result** column and the console list every file: OK, Warning, Error or Skipped with the reason, then a summary line: files, ok, with warnings, errors, skipped, not run, and the time.

A file that cannot be read or compiled is an error for that file only; the run goes on. The game content is mounted again at the end so the new models are in the model browser.

### From the command line

```text
hammerpro --import-models FILE_OR_DIR... --out DIR [options]
```

It needs no window and no game: models go in `DIR/models/hammerpro/` and textures in `DIR/materials/models/hammerpro/<name>/`, a folder you can copy into the game or pack. See [Command line](../reference/command-line.md) for every option. Through MCP the tool is `import_models_batch`.

## Tips

- If the size in the summary looks 39 times too large or too small, toggle **Convert the units** or change **Scale**.
- Keep a model under 32 materials: a game model cannot have more and the import stops with a message.
- To check the result, select it in the model browser: the **Inspector** tab lists its files and materials.
- To turn a terrain model (OBJ, FBX, glTF, GLB, or a prop already in the map) into displacements instead of a prop, use [Mesh to displacement](displacement-tool.md#mesh-to-displacement): it reads the same files, with the same unit and up axis options, and builds a grid of displacement tiles.

## FAQ

**"The game content is not loaded": what now?** Import writes into the game folder of the mounted content. Set the game folder in the settings and wait for the content to load (see [Game setup](../getting-started/game-setup.md)).

**"The model is too large for the game".** Lower its detail in your 3D software and import it again.

**My glTF is refused.** Draco and meshopt compressed meshes are not read: export it without mesh compression.

**Can I import animations or skeletons?** No: only static geometry is read.

## Related pages

- [Model browser](model-browser.md)
- [Blender link](blender.md)
- [Displacement tool: Mesh to displacement](displacement-tool.md#mesh-to-displacement)
- [Propper](propper.md)
- [Entity tool](entity-tool.md)
- [Build and save as content](build-content-release.md)
- [Custom texture folders](custom-texture-folders.md)
- [Script commands](../reference/script-commands.md)

---
[Back to the documentation index](../README.md)
