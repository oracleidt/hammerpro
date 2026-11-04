[Documentation](../README.md) > Getting started > Your first map | [Français](../../fr/getting-started/first-map.md)

# Your first map

A step by step guide: a closed room with a light and a player start, a texture on the walls, then save, compile and play it in Garry's Mod. It takes about five minutes. The keys below are the Hammer Pro defaults; if you chose Hammer or Hammer++ in the setup assistant, see the note in step 8.

## Before you start

- Garry's Mod is set up (see [Game setup](game-setup.md)). Wait until the loading card over the views disappears: the textures and entity definitions are then available.
- Know how to move in the views: in the 3D view, hold the right mouse button and use `W` `A` `S` `D` (`Shift` faster, `Q`/`E` down and up; on an AZERTY keyboard the same key positions). In the 2D views, the mouse wheel zooms and the middle button (or `Space` + left drag) pans. See [Camera and navigation](../features/camera-navigation.md).

## 1. Start a new map

**File > New map** (`Ctrl+N`). An empty, untitled map opens. The four views show 3D (top left), Top, Front and Side.

## 2. Draw a block

1. Pick the **Block tool**: `Shift+B`, or its icon in the tool palette on the left. The options row under the toolbar shows the primitives; keep **Block**.
2. In the **Top** view, drag a rectangle about 512 by 512 units. The grid size is shown in the toolbar; `]` makes the grid larger and `[` smaller.
3. A pending box appears in every 2D view. In the **Front** view, drag its top handle up to make it about 256 units tall (a new box starts 64 units tall).
4. Press `Enter` to create it (`Escape` cancels).

The new brush is selected and its faces use the current material, `dev/dev_measuregeneric01b` by default (the gray measuring texture of the dev set). You can also draw the base directly on a surface in the 3D view, then move the mouse to set the height and click. More in [Block tool](../features/block-tool.md).

## 3. Hollow it into a room

With the block selected, choose **Tools > Make hollow** (`Ctrl+H`). The dialog asks for the **Wall thickness**: keep 32 and press `Enter`. The block becomes six walls, grouped and selected. A negative thickness would build the walls outside the block.

A sealed room matters: a hole to the void makes the map leak (see [Leaks and pointfiles](../features/leaks-pointfiles.md)).

## 4. Put a texture on the walls

With the walls still selected:

1. Open the **Textures** tab of the right dock.
2. Type a word in the search field, for example `concrete`.
3. Double click a material: it is applied to every face of the selected objects.

You can also drag a material from the browser onto a single face in the 3D view, or select faces with **Texture application** (`Shift+A`) for finer work. See [Texture browser](../features/texture-browser.md) and [Texture application](../features/texture-tool.md).

## 5. Add the player start

1. Pick the **Entity tool** (`Shift+E`). The **Class** field in the options row starts with `info_player_start`.
2. Fly the 3D camera into the room, then click the floor. The entity rests on the surface you click; a ghost box with the class name follows the cursor before you click.

In a 2D view, a click places the entity on the grid point under the cursor instead.

## 6. Add a light

1. Still with the Entity tool, click the **Class** field, type `light` and pick **light** in the list.
2. Click the ceiling of the room in the 3D view: the light hangs from it.

Select the light with the **Selection tool** (`Shift+S`) and open the **Entity** tab to change its brightness and color (see [Entity properties](../features/entity-properties.md)).

## 7. Save

**File > Save** (`Ctrl+S`). Choose a folder (the **Map folder** of **Settings > Game** is proposed when set) and a name in lowercase letters, digits and `_`, for example `gm_firstroom.vmf`: the compiled map takes this name. A `.vmx` backup of the previous version is kept next to the map at each save, like Hammer.

## 8. Compile and run

Press `F9` (**File > Compile now**). The map compiles with the last compile settings (the **Normal** preset by default): geometry, visibility, lighting, then the `.bsp` is copied to `garrysmod/maps` and Garry's Mod starts on your map. The status bar shows the step and progress; click it to open the compile window.

To see the options first, use **File > Compile and run...** (`Ctrl+F9`) or the **Compile** button of the toolbar: the **Compile and run** window opens with the presets (**Draft**, **Normal**, **Final**), the steps and **Launch the game**. Click **Compile** there.

With the Hammer or Hammer++ shortcut preset, the keys are swapped as in Hammer: `F9` opens the compile window and `Ctrl+F9` compiles at once.

If Garry's Mod is already running, the map loads in the running game ("Game ready" in the status bar) instead of starting a new one.

## 9. If something goes wrong

- **Leak**: the status bar shows a **Leak** badge and the leak path is drawn in the views. Seal the hole with a world brush and compile again. See [Leaks and pointfiles](../features/leaks-pointfiles.md).
- **Errors in the log**: in the compile window, click an error to go to the brush or entity it names. See [Compile limits and logs](../compiler/limits-and-logs.md) and [Common errors](../troubleshooting/common-errors.md).
- **Undo**: `Ctrl+Z` undoes any step, `Ctrl+Y` redoes it.

## Next steps

- [Compile and test in Garry's Mod](../guides/compile-and-test-gmod.md)
- [Lighting](../guides/lighting.md)
- [Selection tool](../features/selection-tool.md)
- [Compile and run window](../compiler/compile-window.md)

## Related pages

- [Interface tour](interface-tour.md)
- [Block tool](../features/block-tool.md)
- [Entity tool](../features/entity-tool.md)
- [Brush operations](../features/brush-operations.md)

---
[Back to the documentation index](../README.md)
