[Documentation](../README.md) > Guides > Make a prop with Propper | [Français](../../fr/guides/make-a-prop-with-propper.md)

# Make a prop with Propper

Propper turns brushes (and displacements) into a real `.mdl` model with levels of detail and optional collision, placed as a `prop_static` or a movable `prop_physics` where the brushes were. This guide builds a prop, then shows the way back (a prop turned into brushes).

## Before you start

- The game folder is set (**Tools > Options...**, page **Game**). Propper compiles the model with the game's own `studiomdl`, found in its `bin` folder.
- Save your map once: the model is named after the map file.
- Automated runs (`--script`) never compile models, because `studiomdl` writes into the game.

## Step 1: build the shape

1. Build the object with brushes: block tool, clip tool, vertex tool, and displacements for organic parts.
2. Texture every visible face. Faces with a tool texture (`tools/toolsnodraw`...) are left out of the visible model, but still collide. In a displacement brush, only the displaced side is kept, like in game.
3. Keep it reasonable in size: it becomes one model.

## Step 2: check the options

Open **Tools > Prop options (Propper)...**. The LOD settings are shared with model import (FBX, OBJ, glTF); physics and collision choices apply to **Convert brushes to a prop**:

| Option | What it does | Default |
|---|---|---|
| **Levels of detail (LOD)** | Simplified versions of the model that the game draws far away (fewer triangles, same look). | on |
| **Levels** | Levels below the full model, 3 to 6 (big models get one or two more). | 4 |
| **Each keeps** | Part of the triangles of the level above that each level keeps, 0.2 to 0.9 (small models are reduced more gently). | 0.5 |

The window also reminds you how the model is built:

- Collision: each brush stays one convex piece; displacements get slabs that follow their surface.
- Blend materials (WorldVertexTransition) of displacements are baked into a texture of the model.

## Step 3: convert

1. Select the brushes (and displacements) of the object.
2. Choose **Tools > Convert brushes to a prop**.
3. In the creation window, choose Movable physics prop, collision, mass and LOD options, then click Create prop.
4. The console shows the preparation (blend textures, levels of detail, collision), then the `studiomdl` compile.
5. When it is done, the brushes are replaced by a `prop_static` or `prop_physics` using `models/hammerpro/<map>_<id>.mdl`, in one undo step: `Ctrl+Z` brings the brushes back.

The model is written into the game's `models\hammerpro\` folder, so you can test the map right away. When you publish, **Build and save as content** collects it with the map (see [Collect content for a release](collect-content-for-release.md)).

## The way back: prop to brushes

To edit a prop as brushes (yours or any model of the game):

1. Select one `prop_static`, `prop_dynamic` or `prop_physics` with a model.
2. Choose **Tools > Convert prop to brushes...**. The window shows the model, its number of render triangles and its collision model.
3. Choose the options and click **Convert** (`Enter`):

| Option | Choices | Default |
|---|---|---|
| **Shape from** | **Automatic**: the collision model when the model has one (the exact brushes of a prop made by Hammer Pro), else the render mesh. **Collision model**: one brush per convex piece of the collision model. **Render mesh (convex pieces)**: brushes grown from the convex parts of the visible mesh, close to the model but approximate on curved or hollow shapes. | Automatic |
| **Put the brushes in** | `func_detail` or **The world**. | `func_detail` |
| **Keep the prop** | Keep the prop next to the new brushes. | off |

The faces take the materials and texture mapping of the model. The conversion is one undo step. Very detailed models are refused (too many triangles or pieces).

## FAQ

**"studiomdl was not found in the game's bin folder".** Check the Garry's Mod folder on the **Game** page of the settings; `studiomdl.exe` ships with the game in `bin`.

**The prop looks blurry from far away.** That is a level of detail. Raise **Each keeps** or lower **Levels**, or turn **Levels of detail (LOD)** off, then convert again.

**Can I get my brushes back later?** Yes: `Ctrl+Z` right after, or **Convert prop to brushes...** with **Shape from** set to **Automatic**: a Propper prop gives back its exact brushes.

## Related pages

- [Propper](../features/propper.md)
- [Model import](../features/model-import.md)
- [Model browser](../features/model-browser.md)
- [Displacement tool](../features/displacement-tool.md)
- [Collect content for a release](collect-content-for-release.md)

---
[Back to the documentation index](../README.md)
