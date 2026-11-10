[Documentation](../README.md) > Troubleshooting > Out of memory | [Français](../../fr/troubleshooting/out-of-memory.md) | [Русский](../../ru/troubleshooting/out-of-memory.md)

# Out of memory

Two kinds of memory can run out: the memory of the graphics card (VRAM), which holds the geometry, textures, models and lightmaps of the maps you see, and the memory of the computer (RAM), which holds the maps themselves, the undo history and the content lists. Hammer Pro watches both and frees what it can before anything breaks.

## Graphics memory (VRAM)

### What Hammer Pro does on its own

Hammer Pro measures the graphics memory it uses (Windows reports it for the program; on other systems it is estimated from what Hammer Pro created) and compares it with a **budget**. Every half second:

| Use of the budget | What happens |
|---|---|
| Below 85% | Nothing. The maps of the other tabs stay on the graphics card, so switching tabs is instant, as long as they fit in three quarters of the budget with the map you see. |
| Above 85% | The maps parked in the other tabs are freed from the graphics card (they load again when you show their tab), textures and models no open map uses are freed, and the baked lighting preview is freed when no view shows it (it is baked again when you show it). A warning says so once. |
| Above the budget, or after an out of memory error | Textures are loaded again at half their size, one step every 2 seconds, down to 256 pixels, and the thumbnails of the browsers are freed. The warning says that textures are shown at a lower resolution. |
| Below half the budget for 20 seconds | Textures come back to their full size, one step at a time. |

When the graphics card refuses an allocation, Hammer Pro keeps the old buffer instead of drawing with a broken one, frees memory, then loads the map on the card again: the view never stays empty or broken, it only shows fewer details for a moment.

Leaving the player view frees the compiled world, its lightmaps and its sky. Baking the lighting again replaces the previous bake instead of adding to it, and the bake is sent to the graphics card in pieces of 32 MB.

Before any of this, **Load texture detail by distance** (`render.texture_streaming`, on by default) keeps textures used only far from every view at a smaller size that looks the same at that distance, so big maps need less graphics memory in the first place. Textures may use 60% of the budget; when they would use more, the farthest ones are reduced one more level before the steps above lower every texture. See [Editor performance](performance.md#texture-detail-by-distance).

### The budget

**Settings > Render > Graphics memory**:

| Setting | What it does |
|---|---|
| **Graphics memory budget** | How much graphics memory Hammer Pro may use. **Automatic** (0, the default) uses 90% of what Windows gives the program, or 3 GB (1.5 GB on integrated graphics) when the driver does not tell; on integrated graphics, which use the system memory, never more than 2 GB. Set a value in MB to leave room for the game or another program running at the same time. |
| **Keep other tabs on the graphics card** | On (default): switching tabs is instant while there is room. Off: only the map you see stays on the graphics card, the others load again when shown. |
| **Free graphics memory now** | Frees the maps of the other tabs, unused textures and models, and the thumbnails at once. |

The line under these settings shows the memory in use and its source ("measured by the driver" or "estimated").

The same settings are stored as `vram_budget_mb` and `keep_background_maps_on_gpu` in the `[render]` table of `settings.toml`. Through MCP: `set_setting` with `render.vram_budget_mb`, `get_gpu_memory` for the measures, `free_gpu_memory` to free memory now. Scripts can print the measures with the `vram` command.

### When the warning keeps coming back

- Close the tabs you do not use: every open map costs graphics memory as long as it is on the card.
- Turn off **Keep other tabs on the graphics card**.
- Close the game or other 3D programs while editing a very large map.
- Lower the budget if another program needs the graphics card at the same time.
- A huge mounted content folder does not cost graphics memory by itself: only the textures and models of the open maps are loaded.

## Computer memory (RAM)

Hammer Pro keeps a small reserve of memory. When the computer runs out of memory, the reserve is given back so Hammer Pro can go on, your modified maps are saved at once to the autosave folder, and a message asks you to close other programs or some tabs. The reserve is taken again 30 seconds later when there is room.

If an allocation still fails (one single request bigger than all the free memory), Hammer Pro writes a crash report ("memory allocation of N bytes failed") and, when the failure happened in a background task, saves the modified maps before closing. See [Crashes and logs](crashes-and-logs.md).

## Related pages

- [Graphics device lost](graphics-device-lost.md)
- [Crashes and logs](crashes-and-logs.md)
- [Editor performance](performance.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
