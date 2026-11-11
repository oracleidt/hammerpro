[Documentation](../README.md) > Features > Trackpad | [Français](../../fr/features/trackpad.md) | [Русский](../../ru/features/trackpad.md)

# Trackpad

Hammer Pro works on a laptop trackpad without a mouse: two finger scrolling moves the views instead of zooming them, pinching zooms, and `Space` + drag replaces the middle mouse button. The settings are on the **Input** page of the settings (**Tools > Options**, section Application).

## Trackpad mode

**Trackpad mode** (`[input] trackpad_mode`) has three values:

| Value | Behaviour |
|---|---|
| **Auto** (default) | Turns the trackpad gestures on as soon as a trackpad gesture is seen (a smooth or sideways scroll, a pinch), and off again at the first notch of a mouse wheel. The page shows what was detected. A plain mouse wheel never turns it on. |
| **On** | Every scroll is read as a trackpad gesture, even a mouse wheel. Use it when Auto does not recognise your trackpad. |
| **Off** | The mouse behaviour, exactly as before: the wheel zooms, the middle button pans. |

With a high resolution mouse wheel that sends sideways fractions, Auto may take it for a trackpad: choose **Off**.

## Gestures

| Gesture | 2D views | 3D view |
|---|---|---|
| Two finger scroll | Pan the view (the map follows the fingers) | Orbit the camera around the point at the center of the view, or move the camera sideways and up or down (**Two finger scroll in 3D**) |
| Pinch, or `Ctrl` + scroll | Zoom at the cursor (toward the cursor when **Zoom toward the cursor** is on) | Move the camera forward or back |
| `Alt` + scroll | Acts as the mouse wheel: the tool uses it first (block count, curve segments...), otherwise it zooms | Acts as the mouse wheel (moves the camera) |
| `Shift` + scroll | Acts as `Shift` + mouse wheel | Acts as `Shift` + mouse wheel (faster camera move) |
| `Space` + drag | Pan the view | Look around |
| `Space` + `Shift` + drag | | Pan the camera |
| Tap | Click; a little finger motion (up to 9 points) still counts as a click | Same |
| Two finger tap | Right click: context menu | Same |
| Scroll while flying (right button held or mouselook) | | Changes the camera speed, as the wheel does |

- `Ctrl` + scroll with a real mouse wheel still changes the grid size (or zooms all 2D views, **Ctrl + wheel in 2D**) when the trackpad gestures are off. With **On**, `Ctrl` + scroll zooms; the grid keys `[` and `]` change the grid.
- Scrolling is smooth: fractional deltas add up, and one frame never pans more than 240 points or zooms more than 3 wheel notches, so a fast flick cannot jump across the map.
- In the 3D view the gestures work with every control scheme. `Space` + drag in the 3D view looks around even with the Hammer Pro scheme while the trackpad gestures are on.
- Lists, panels and dialogs scroll with two fingers as usual in every mode.

## Settings

| Setting | Default | Effect |
|---|---|---|
| **Trackpad mode** (`input.trackpad_mode`) | `Auto` | `Auto`, `On` or `Off` |
| **Two finger scroll in 3D** (`input.trackpad_scroll_3d`) | `Orbit` | `Orbit` turns around the point at the view center, `Move` slides the camera |
| **Scroll speed** (`input.trackpad_pan_speed`) | 1.00 | 0.25 to 4, multiplies the pan |
| **Pinch zoom speed** (`input.trackpad_zoom_speed`) | 1.00 | 0.25 to 4, multiplies pinch and `Ctrl` + scroll zoom |
| **Orbit speed** (`input.trackpad_orbit_speed`) | 1.00 | 0.25 to 4, multiplies the 3D orbit |

## Through MCP

- `set_setting` with `input.trackpad_mode` (`"Auto"`, `"On"`, `"Off"`), `input.trackpad_scroll_3d` (`"Orbit"`, `"Move"`) and the three speeds; `get_settings` reads them.
- `ui_scroll` with `trackpad: true` sends a two finger scroll of `dx`, `dy` points; with `pinch` (for example 1.25) it sends a pinch. Example: `{"view": "top", "trackpad": true, "dx": 40, "dy": 0}` pans the top view when the trackpad gestures are on.
- Scripts: `trackpad <pos> <dx> <dy> [mods]` and `pinch <pos> <factor>` (see [Script commands](../reference/script-commands.md)).

See also: [Camera and navigation](camera-navigation.md), [Keyboard shortcuts](keyboard-shortcuts.md), [Settings dialog](settings-dialog.md).
