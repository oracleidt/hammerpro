[Documentation](../README.md) > Features > Entity tool | [Français](../../fr/features/entity-tool.md) | [Русский](../../ru/features/entity-tool.md)

# Entity tool

The entity tool places point entities: on surfaces in the 3D view (resting on floors, walls, ceilings and on prop models), or on the grid in the 2D views. It also places prefabs chosen in the Prefabs tab.

A model fixed by the entity definition, or supplied as an FGD model default, takes precedence over the model browser. Its skin is also preserved; the browser skin applies only to generic props using the selected browser model.

## Opening it

- **Tools > Entity tool**, the tool palette, or `Shift+E`.
- When no class is chosen yet, the tool uses the **Default point entity** setting (`info_player_start`).

## Placing an entity

- **3D view**: click a surface. The entity box rests on it from the outside: on a floor it stands on it, on a ceiling it hangs from it, on a wall it touches the wall. On a prop, the entity rests on the real mesh of the model (not on its box); where the model has no triangle, it falls through to the surface behind. With no surface under the cursor, the entity goes on the ground plane (height 0).
- **2D views**: click to place the entity on the grid point under the cursor. Its depth (along the view axis) is the origin of the last entity placed in 3D, else the center of the selection, else 0.
- A ghost box with the class name follows the cursor before you click.

Modifiers:

| Key | Effect |
|---|---|
| `Shift` + click | Keep the current selection (otherwise the new entity becomes the selection). |
| `Alt` + click | No grid snapping (the resting offset is then rounded to 0.01 unit). |

Positions snap to the grid of the view; on a brush that carries its own grid, the entity lands on that grid (see [Grids](grids.md)). The resting offset is rounded to whole units (or to the grid when it is finer than one unit), so origins stay clean.

Each placement is one undo step. The new entity gets the FGD default keyvalues and spawnflags of its class.

### Size of the ghost box

The box used to rest the entity on surfaces is, in this order: the collision hull of the model (for props, and for classes with a fixed model and no size, like `info_player_start`, which uses the 32 x 32 x 72 player hull), else the model bounds, else the size given in the FGD, else a 16 unit cube.

## Options

| Option | What it does | Default |
|---|---|---|
| **Class** | Searchable class picker (see below). Its tooltip shows the FGD description of the class. Without loaded entity definitions it becomes a plain text field (**Class name**). | **Default point entity** setting |
| **Model** | Only for generic prop classes (`prop_*` or a studio prop helper, without a fixed model or a nonempty FGD model default): the model chosen in the model browser, or **No model selected**. | |
| **Skin N** | Skin chosen in the model browser, written to new props (shown when it is not the default skin). | |
| **Models...** | Opens the Models tab of the model browser to choose the model. | |
| **Align to surface** | Prop classes only: rotate props so their up axis follows the surface normal (the `angles` keyvalue is set). | off |

A prop placed with no model selected is created without a model and a warning is written to the console.

## The class picker

Click the class button to open the list:

- Type in **Search classes...**: every word must appear in the class name; names starting with the first word come first.
- `Up` and `Down` move in the list, `Enter` picks the highlighted class, `Escape` closes the list.
- **Recent**: the last classes you placed (up to 6 shown) are listed first when the search is empty.
- **Use "name"**: a class name that is not in the FGD (Garry's Mod scripted entities, for example) can still be used.
- Each row shows the editor color of the class, and its description as tooltip. The bottom line counts the matching classes.

## Placing prefabs

Double click a prefab in the **Prefabs** tab: the entity tool switches to prefab placement. The options bar then shows **Prefab:** with its name, a **Back to entities** button and the **Group** checkbox.

| Gesture | Result |
|---|---|
| Click in the 3D view | Insert the prefab at its original size, resting on the surface under the cursor. |
| Click in a 2D view | Insert it centered on the grid point (at the depth the entity tool uses). |
| Drag a box in a 2D view | Insert it scaled to the box drawn. |
| `Escape` | Drop the box being drawn, else go back to placing entities. |

Offsets snap to the grid, so prefabs built on the grid stay on it. Each insertion replaces the `&i` token of the prefab by the next number of the map. **Group** (setting `[placement] group_prefabs`, on by default) groups the objects of each inserted prefab. See [Prefabs](prefabs.md) for the library.

## Tips

- Pick the model in the [Model browser](model-browser.md) first, then click with the entity tool: each click places a new prop with that model and skin.
- Use `Shift` + click to place a row of lights without losing the selection you are working on.
- To edit keyvalues after placing, open the [Entity properties](entity-properties.md) (`Alt+Enter`).
- The MCP tools `create_entity` and `set_current_entity_class` do the same from an AI assistant (see [MCP tool reference](../../MCP.md)).

## FAQ

**Why is my entity floating above the floor?** The tool rests the box of the entity on the surface; for a model it uses the collision hull, which can be larger than the visible mesh. Use `Alt` + click or move it afterwards.

**My custom Lua entity is not in the list.** Type its name in the search field and pick **Use "name"**.

**Why does a prop have no model?** No model was chosen in the model browser. Click **Models...**, pick one, then place again.

## Related pages

- [Entity properties](entity-properties.md)
- [Model browser](model-browser.md)
- [Prefabs](prefabs.md)
- [Path tool](path-tool.md)
- [Sprinkle tool](sprinkle-tool.md)
- [Decals and overlays](decals-overlays.md)
- [Grids](grids.md)

---
[Back to the documentation index](../README.md)
