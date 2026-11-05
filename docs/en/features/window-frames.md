[Documentation](../README.md) > Features > Window frames | [Français](../../fr/features/window-frames.md)

# Window frames and separate windows

Hammer Pro draws its own window frame: a title bar that holds the menus, rounded corners, a thin border and a soft shadow. If the frame gives you trouble (remote desktop, some window managers), you can switch to the frames of your system. Some tools can also leave the main window and live in windows of their own, on another screen for example.

## The Hammer Pro frame

The main window has no system title bar. Its own title bar shows the menus, the title of the window (`<map> - Hammer Pro`) and three buttons: **Minimize**, **Maximize** (or **Restore**) and **Close**.

| Gesture | What it does |
| --- | --- |
| Drag the title bar | Moves the window. Snapping to the screen edges keeps working (the system runs its own move loop). |
| Double click the title bar | Maximizes or restores the window. |
| Drag an edge or a corner | Resizes the window. |

A maximized window has square corners and no shadow. When the graphics card cannot draw a transparent window, the frame is square, without shadow.

## Native window frames

**Settings > General > Native window frames** uses the title bars and frames of the system instead of Hammer Pro's, for remote desktop or window managers that have trouble with them. It applies at once, both ways, to the main window and to the separate windows. Key `ui.native_frames`, off by default.

## Maximized startup and restore

Hammer Pro starts maximized in the Windows work area. The taskbar remains accessible, and the minimize, maximize/restore and close buttons stay visible. `F11` (**View > Maximize window**) switches between maximized and restored, restoring the previous window position and size. This is normal window maximization within the work area.

**Settings > General > Start maximized** is on by default and applies at the next start. Its historical configuration key remains `ui.start_fullscreen`, and the action identifier remains `toggle_fullscreen` for compatibility; both now control maximization and restore. Fresh runtime verification of the work-area bounds and restore behavior is pending.

## Separate windows

These windows can leave the main window and become windows of the system, which you can put on another screen and keep open while you work:

| Window | How to detach it | Default |
| --- | --- | --- |
| **Compile and run** | **Separate window** button of the window ("Show this window as a window of its own... Remembered for the next times"). Setting `compile.separate_window`. | separate window |
| **UV editor** | **Open as a separate window** in the UV editor. | inside the panel |
| **Portals and visibility** | **Detach window** in the window. | inside the main window |

A separate window wears the Hammer Pro frame with the Hammer Pro mark, its title and three buttons: **Put back in Hammer Pro** (back inside the main window), **Maximize** and **Close**. The Compile and run and Portals windows also have a **Put back in the main window** button in their content.

Separate windows belong to the main window: they stay above it (not above every other program) and hide with it when it is minimized.

### Placement

Where each separate window was last is remembered in pixels of the desktop (`compile.window`, `ui.uv_window`, `ui.portals_window`). When it opens again:

- it goes back to the same screen, and moves with that screen if you arranged your screens again;
- it stays as it was while its title bar is on a screen (a window across two screens stays across them);
- if its screen is gone or it is off every screen, it is brought back inside the screen it overlaps most (else the nearest one) and made smaller when it does not fit.

On systems other than Windows, the window manager places the windows; Hammer Pro reads the placement from the interface and restores it as well as it can.

## Tips

- With two screens, keep the Compile and run window on the second one and keep editing while the map compiles.
- If the window shows a black or glitchy frame over remote desktop, turn on **Native window frames**.

## FAQ

**I cannot find the minimize button.** It is at the right of the title bar, next to **Maximize** and **Close**. With native frames, the system's own buttons are used.

**A separate window opened off screen.** Its placement is brought back on a screen automatically. If it still happens, put it back in the main window with its **Put back** button, then detach it again.

**Do dialogs like Settings become separate windows?** No. Only the windows listed above can be detached; the other dialogs float inside the main window.

## Related pages

- [Themes and languages](themes-languages.md)
- [Compile window](../compiler/compile-window.md)
- [UV editor](uv-editor.md)
- [Leaks and pointfiles](leaks-pointfiles.md)
- [Crashes and logs](../troubleshooting/crashes-and-logs.md)

---
[Back to the documentation index](../README.md)
