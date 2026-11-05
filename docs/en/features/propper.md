[Documentation](../README.md) > Features > Propper | [Français](../../fr/features/propper.md) | [Русский](../../ru/features/propper.md)

# Propper

Propper turns selected brushes and displacements into a compiled model, with optional collision, levels of detail and baked blend materials, and replaces them with a `prop_static` or a movable `prop_physics`. Unpropper does the reverse: it rebuilds a prop as editable brushes.

## Opening it

| Command | Where | What it needs |
|---|---|---|
| **Convert brushes to a prop...** | **Tools** menu, and a button of the tool palette | Selected brushes (world brushes or brushes of entities). Greyed out otherwise: "Select the brushes to turn into a prop". |
| **Convert prop to brushes...** | **Tools** menu, and a button of the tool palette | A selected `prop_*` entity whose `model` is a `.mdl`. Greyed out otherwise: "Select a prop to turn back into brushes". |
| **Prop options (Propper)...** | **Tools** menu | Nothing. |

None of them has a default shortcut (bind one in **Settings > Keyboard**).

## Convert brushes to a prop

### Creation window

**Convert brushes to a prop...** first opens a confirmation window. Choose the options, then click **Create prop** (or Enter). **Cancel** or Escape closes the window before creating files or replacing brushes.

| Option | Effect | Default |
|---|---|---|
| **Movable physics prop** | Creates a `prop_physics` model and entity; off creates a fixed `prop_static`. | Off |
| **Generate collision model** | Generates convex collision geometry. Turning it off creates a non-colliding static decoration. Physics props require it. | On |
| **Mass (kg)** | Compiled physical mass, from 0.1 to 50000 kg; shown for physics props. | 10 |
| **Levels of detail (LOD)** | Enables LOD generation and controls level count/reduction. | On, 4 levels, 50% |

The global **Prop options (Propper)...** window contains the same defaults. The model import shares the LOD options; its own physics controls remain in the model importer. Creation settings are saved as `lab.propper_physics`, `lab.propper_collision` and `lab.propper_mass`.

Through MCP, use `set_setting` to configure these keys and the `lab.model_lod` keys, select the brushes, then `run_action` with `brushes_to_prop`. Confirm the creation window with `ui_key` Enter or `ui_click` on Create prop.

### Before you start

- Garry's Mod must be configured (**Settings > Game**) or detected, and its studiomdl must exist (`bin/win64/studiomdl.exe`, else `bin/studiomdl.exe`): Propper compiles with it. Messages tell you when the game folder or studiomdl is missing.
- Select the brushes and displacements that make the prop. Only their visible faces count; a selection without any gives "The selection has no visible faces to make a prop from".

### What happens

1. The geometry is collected. Faces keep their material and texture mapping (flat shaded, like brushes). Displacements keep their smooth normals; like in game, only the displaced side of a displacement brush exists.
2. The surface property of the model is the dominant one of the materials used.
3. Blend materials (`WorldVertexTransition`) of displacements cannot be used by a model: each such displacement gets a rectangle of a texture atlas (at most 4096 texels wide) where both textures are mixed by the painted alpha like the shader does (with `$blendmodulatetexture`, the `$detail` texture, and the bump maps mixed into a normal map). A `VertexLitGeneric` material shows the result. The rest of the model keeps its materials.
4. The levels of detail are made from the shared settings (see below).
5. When collision is enabled, the collision model keeps every brush as one convex piece, tool faces included, so the prop collides like the brushes did. Displacements collide too: their surface is cut into slabs hanging 8 units below it, so players walk and props rest on the exact surface.
6. The SMD and QC source files are written into the `propsrc` folder of the Hammer Pro cache, the baked textures into `materials/models/<model>/` of the game folder, then studiomdl compiles `models/hammerpro/<map name>_<code>.mdl` into the game (an unsaved map is named `map`). The console shows its output.
7. The content is mounted again and the brushes are replaced by a `prop_static` or `prop_physics` at the center of their bounds, according to the chosen type (one undo step). Brush entities left empty are removed.

The toast and the console tell the result: levels of detail with their triangle counts, collision pieces, surface property, and the baked atlas size when there was one.

## Prop options (Propper)

This window holds the options of "Convert brushes to a prop". The [model import](model-import.md) shares the LOD settings in `[lab.model_lod]`. Physics and collision options apply to brush conversion.

| Option | What it does | Default |
|---|---|---|
| **Levels of detail (LOD)** | Make simplified versions of the model that the game draws far away (fewer triangles, same look). | On |
| **Levels** | Levels below the full model, 3 to 6. Big models get one or two more. | 4 |
| **Each keeps** | Part of the triangles of the level above each level keeps, 20 to 90 %. Small models are reduced more gently. | 50 % |

The levels switch at distances that double each level. The simplification keeps texture seams, material borders, holes and the silhouette: flat areas are reduced first. The window also recalls how collision and blend materials are handled. **Close** or `Escape` closes it.

## Convert prop to brushes (Unpropper)

The **Convert prop to brushes** window works on the first selected prop (`prop_static`, `prop_dynamic`, `prop_physics` or any `prop_*` with a `.mdl` model). It reads the model through the game content, with the body groups of its `body` key, the materials of its `skin`, and its origin, angles and model scale. The window shows the model path, its render triangle count, and whether it has a collision model ("Collision model: N convex pieces (one brush each)" or "No collision model: the brushes are built from the render mesh").

| Option | What it does | Default |
|---|---|---|
| **Shape from** - **Automatic** | The collision model when the model has one (the exact brushes of a prop made with Hammer Pro), else the render mesh. | Selected |
| **Shape from** - **Collision model** | One brush per convex piece of the collision model. Refused for a model without one. | |
| **Shape from** - **Render mesh (convex pieces)** | Brushes grown from the convex parts of the visible mesh: close to the model, approximate on curved or hollow shapes. | |
| **Put the brushes in** | **func_detail** (a new one), or **The world** (grouped when there are several brushes). | func_detail |
| **Keep the prop** | Leaves the prop in the map next to the brushes. | Off |

These choices are saved (`unprop_shape`, `unprop_detail`, `unprop_keep_prop` in `[lab]`). **Convert** (`Enter`) runs it, **Cancel** (`Escape`) closes the window.

Each brush face takes the material and texture alignment of the model triangles lying on it; faces the model does not draw (hidden faces, the nodraw and tool faces of the original brushes) get `tools/toolsnodraw`. The new brushes are selected, the prop is removed unless you keep it, all in one undo step. A toast tells how many brushes, textured faces and nodraw faces were made, how many brushes only approximate the model, and how many parts were too small or too flat to become a brush.

Limits: at most 20000 render triangles to rebuild from, and at most 1024 brushes per model.

## Tips

- A prop made by Propper with collision enabled converts back to its exact original brushes with **Automatic** or **Collision model**: Propper's collision model is made of them.
- Put the props you make in a content pack with **Build and save as content**: the models are in `models/hammerpro/` of the game (see [Build and save as content](build-content-release.md)).
- Through MCP, `run_action brushes_to_prop` and the `prop_to_brushes` tool do the same, and the LOD options are the settings `lab.model_lod.enabled`, `lab.model_lod.levels` and `lab.model_lod.reduction` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**Does conversion overwrite source VMTs?** Ordinary brush material names are retained. Blended displacement materials produce an atlas and a dedicated `VertexLitGeneric` VMT under `materials/models/hammerpro/<model>/blend.vmt`. Those generated files belong to the model export.

**"studiomdl was not found in the game's bin folder".** Propper compiles with the studiomdl of Garry's Mod: check the game folder in **Settings > Game**. The [model import](model-import.md) does not need it.

**The prop compiled but the brushes are still there.** The console shows the studiomdl output; "The model compiled but the brushes could not be replaced" gives the reason.

**Unpropper says "No part of the model is large enough to make a brush".** The model is made of thin or tiny parts; try another **Shape from** choice.

**Does it work in automated runs (`--script`)?** No: studiomdl writes into the game, so automated runs do not compile models.

## Related pages

- [Make a prop with Propper](../guides/make-a-prop-with-propper.md)
- [Model import](model-import.md)
- [Model browser](model-browser.md)
- [Displacement tool](displacement-tool.md)
- [Brush operations](brush-operations.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
