[Documentation](../README.md) > Troubleshooting > Graphics device lost | [Français](../../fr/troubleshooting/graphics-device-lost.md) | [Русский](../../ru/troubleshooting/graphics-device-lost.md)

# Graphics device lost

The graphics device is lost when the graphics driver resets the card (Windows does it when the card does not answer for about 2 seconds), when the card runs out of memory in the middle of a frame, when the driver crashes or when the card is removed. Nothing can be drawn any more with that device.

## What Hammer Pro does

1. It writes a crash report with the reason and the last lines of the log to `%APPDATA%\HammerPro\crash\`.
2. It saves every modified map (the tab you see and the others) next to the autosaves, in `%LOCALAPPDATA%\HammerPro\autosave\`.
3. It shows **Graphics device lost** with the reason and asks **Restart Hammer Pro now?**
   - **Yes**: Hammer Pro starts again and opens all your tabs again, the modified maps from their saved changes. A message says that the maps were opened again.
   - **No**: Hammer Pro closes. At the next start, the crash window shows the report and offers **Open the saved changes**.

Your work is never lost by a device loss: at worst you get the saved changes back in a new tab marked as modified, to save over the original map.

## What Hammer Pro avoids

- **Slow frames of other programs.** With Vulkan on Windows, a frame that the screen did not show within one second used to count as a lost device. Hammer Pro now waits longer instead (up to 30 seconds), so a game, a compile on the graphics card or a long frame of the editor no longer stops it.
- **Out of memory.** Hammer Pro keeps its use of graphics memory under a budget and frees memory before the card is full. See [Out of memory](out-of-memory.md).
- **Huge single uploads.** Baked and compiled lightmaps are sent to the card in pieces of 32 MB, so no single transfer keeps the card busy for long.

## When it happens again

| What you see | What to try |
|---|---|
| The device is lost when opening a big map or entering the player view | Update the graphics driver. Lower **Settings > Render > Graphics memory budget**, close the game and other 3D programs, close the tabs you do not use. |
| The device is lost after a while of editing | Watch the graphics memory line of **Settings > Render**: if it is near the budget, see [Out of memory](out-of-memory.md). |
| The device is lost at start | Force another graphics API with the `WGPU_BACKEND` environment variable (`dx12` or `vulkan`), see [Crashes and logs](crashes-and-logs.md#choosing-the-graphics-api). |
| A laptop with two graphics cards | Make Hammer Pro use the dedicated card in the Windows graphics settings or the driver panel. |

Send the crash report with the log file when it keeps happening, see [Reporting bugs](reporting-bugs.md).

## Related pages

- [Out of memory](out-of-memory.md)
- [Crashes and logs](crashes-and-logs.md)
- [Autosave and crash recovery](../features/autosave-crash.md)

---
[Back to the documentation index](../README.md)
