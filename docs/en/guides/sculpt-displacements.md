[Documentation](../README.md) > Guides > Sculpt displacements | [Français](../../fr/guides/sculpt-displacements.md) | [Русский](../../ru/guides/sculpt-displacements.md)

# Sculpt displacements

This guide makes a piece of terrain: turn brush faces into displacements, sculpt hills and a river bed, blend two textures, and keep the tiles sewn together. Every option of the tool is listed in [Displacement tool](../features/displacement-tool.md).

## Step 1: make the base

1. With the block tool, draw a flat brush for the ground, for example 1024 x 1024 units and 16 units thick. For a big terrain, use several brushes side by side: each top face becomes one tile.
2. Give it a blend material (a `WorldVertexTransition` material with two textures, such as grass and dirt) if you want to paint the mix of textures later.

## Step 2: create the displacements

1. Select the brushes (or only the faces you want with the texture tool).
2. Press `Shift+P` (**Tools > Active tool > Displacement tool**). The **Tool** panel of the right dock shows the displacement options.
3. In the **Create** section, the line under it tells what is used: the selected faces, or the top faces of the selected solids. Only 4 sided faces can become displacements.
4. Click a **Power** button: **2** (5 x 5 vertices), **3** (9 x 9) or **4** (17 x 17). Power 3 is a good start; use 4 for detailed ground.

## Step 3: paint the shape

1. Choose the **Paint geometry** mode.
2. Choose the effect: **Raise**, **Lower**, **Smooth** or **Raise to** (a target height above the face).
3. In the **Brush** section, set **Radius** (world units), **Strength** (units per second at the brush center) and **Falloff** (**Constant**, **Linear**, **Smooth**).
4. Choose the **Axis**: **Face normal**, **Surface normal**, **Brush center**, **View (camera)**, **View (camera), horizontal** (toward the camera on X and Y only), **X**, **Y**, **Z (up)** or **Custom** (Alt and a right click on a face in the 3D view takes its normal).
5. In the 3D view, hold the left button over the displacements and move: a brush circle follows the surface.
   - `Shift` inverts (lower instead of raise).
   - A right drag, or `Alt` and a horizontal drag, changes the radius.
   - A short click selects a face (`Ctrl` toggles).
   - `Escape` cancels the stroke being painted.
   - Each stroke is one undo step.

With **Continuous** on, holding the button still keeps painting. With displacements selected and **Selected only** on, only the selected ones are painted: press the key shown in the hint to clear the selection and paint everything.

## Step 4: sculpt precise forms

The **Sculpt** mode shapes the ground from where it was when the stroke started:

- **Push**: raises the surface to a layer at an offset (**Distance** in units with **Absolute**, or **Percent** of the radius with **Adaptive**). Going over the same place again does not pile up with **Attenuated**; **Additive** keeps adding. `Ctrl` sinks instead.
- **Carve**: digs a **Profile** (V, U, flat, or your own 5 points) of a given **Depth** along the stroke path: ditches, rivers, roads. `Ctrl` raises a ridge.
- **Project**: moves the surface onto the selected brushes along the sculpt axis (brushes farther than **Range** are ignored). Select the brushes together with the displacements first.

`Shift` smooths while sculpting, with the strength set in **Smoothing**.

## Step 5: keep the tiles together

- **Sew while painting**: neighbouring tiles follow the brush live and the remaining cracks are closed when the stroke ends.
- **Spatial** measures the brush in 3D on the surface, so it also paints walls and across tiles.
- **Line of sight** only paints what the camera sees at the brush (not the terrain behind a hill).
- **Sew** in the **Selection** section closes the cracks between the selected displacements after the fact; **Select adjacent** adds the tiles that share an edge.

## Step 6: paint the texture blend

1. Choose **Paint alpha**.
2. **Texture**: **2-way** paints the alpha between the two textures; with a 4-way blend material, pick the texture (1 to 4) to paint.
3. Choose **Add** (reveals the second texture), **Remove** (the first), **Set value** (0 shows the first texture, 255 the second) or **Smooth**.
4. Paint in the 3D view like the geometry. **Invert alpha** in the **Tools** section swaps the two textures.

## Step 7: finish

In the **Tools** section of the panel:

- **Add noise**: a random height between a minimum and a maximum on every vertex (also **Tools > Displacements > Add noise to displacements...**).
- **Subdivide**: smooths the selected displacements as one surface, then sews them.
- **Scale**: multiplies the heights (a negative factor turns them over).
- **Cast shadows**, **Lightmap scale** and the collision flags (**No hull collision**, **No physics collision**, **No ray collision**) apply to the selected displacements.

Check where players can walk with **View > Diagnostics > Displacement walkability**, and paint the **Tags** mode (walkable, buildable, removed) where the slope gives the wrong answer.

## FAQ

**Nothing happens when I paint.** Only the selected displacements are painted when some are selected: click the tile to select it, or clear the selection.

**Some faces were skipped at creation.** Only 4 sided faces can become displacements. Clip or rebuild the brush so its top face has four sides.

**Cracks between tiles.** Turn **Sew while painting** on, or select the tiles and click **Sew**.

## Related pages

- [Displacement tool](../features/displacement-tool.md)
- [Block tool](../features/block-tool.md)
- [Render modes and display](../features/render-modes-display.md)
- [Make a prop with Propper](make-a-prop-with-propper.md)
- [Script commands](../reference/script-commands.md)

---
[Back to the documentation index](../README.md)
