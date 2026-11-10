[Documentation](../README.md) > Troubleshooting > Crashes and logs | [Français](../../fr/troubleshooting/crashes-and-logs.md) | [Русский](../../ru/troubleshooting/crashes-and-logs.md)

# Crashes and logs

What Hammer Pro does when something goes wrong, where it writes reports and logs, and what to try when the graphics card or the window frame causes trouble.

## After a crash

Hammer Pro protects your work in several ways:

- **Emergency save.** When the editor catches a crash of its frame, it writes an emergency save of the modified map next to the autosaves (`%LOCALAPPDATA%\HammerPro\autosave\`) and records it in the crash report.
- **Crash report.** A crash of the main thread writes a report (message, location, thread, backtrace and the last lines of the log) to `%APPDATA%\HammerPro\crash\`.
- **Native crashes.** A crash outside Rust code (an access violation in the graphics driver, a stack overflow) writes a report too, with the exception, the module it happened in (for example `nvoglv64.dll+0x1234`) and the last lines of the log. When it happens in a background thread, Hammer Pro first saves the modified maps (it waits up to 10 seconds for that).
- **Out of memory.** Hammer Pro keeps a reserve of memory: when the computer runs out, the reserve is given back, the modified maps are saved and a message asks you to free memory. An allocation that still fails writes a report ("memory allocation of N bytes failed"). See [Out of memory](out-of-memory.md).
- **Runs that end without a word.** A killed process, or a crash so deep that even the report cannot be written, leaves nothing at that moment. Each run leaves a marker in the crash folder that a normal close removes; on the next start, a marker left by a dead process becomes a report with the end of the previous run's log.

On the next start, the window **Hammer Pro closed unexpectedly** shows the **Report file:** path and these buttons:

| Button | What it does |
|---|---|
| **Show the report file** | Opens its folder with the report selected. |
| **Copy the report** | Copies the text of the report, to paste it in a message. |
| **Open the saved changes** | Opens the emergency save, when one was written ("Your unsaved changes were saved to ..."). |

Errors of worker threads do not stop the editor: they are logged with their location, and the first few of a run also get a report file whose path is in the log (not offered at the next start).

Automated runs (`--script`, `--screenshot`) never read or write the crash folder.

## Autosaves

Independently of crashes, Hammer Pro saves modified maps in the background every 5 minutes by default and keeps 10 copies per map (**Settings > General > Autosave**: **Save every**, **Copies kept per map**; 0 minutes turns it off). When you open a map that has a newer autosave, **Recover autosave?** offers to open it. See [Autosave and crash recovery](../features/autosave-crash.md).

## The log file

| File | Content |
|---|---|
| `%LOCALAPPDATA%\HammerPro\logs\hammerpro.log` | The current run, written line by line (it survives an abort). |
| `%LOCALAPPDATA%\HammerPro\logs\hammerpro.old.log` | The previous run. |

The log holds the English diagnostics of the libraries (mount timings, file counts, the graphics adapter chosen, the graphics memory budget) as well as every warning and error. Warnings and errors also appear in the **console** panel (`` ` ``), translated.

A message repeated in a burst is written 5 times per 10 seconds at most; the next one written says how many similar messages were hidden. Writing to the log never stops Hammer Pro, even when its output is closed.

## Reports and log through MCP

An AI assistant connected through [MCP](../reference/mcp-server.md) can read them for you: `get_crash_reports` lists the reports of the crash folder (newest first, with their kind: crash, worker thread or a previous run that ended unexpectedly, and the message line) and reads one when given its file name; `get_app_log` returns the last lines of the log of this session, filtered with `contains`. Both only read: they never write, and `get_crash_reports` reads nothing outside the crash folder.

## Graphics problems

### Graphics errors and lost device

A graphics error of one frame (validation, often a driver problem) no longer closes the editor: the frame is skipped and the console shows "Graphics error (one frame was skipped): ..." once (further ones are counted, details in the log file). An out of memory error makes Hammer Pro free graphics memory at once, see [Out of memory](out-of-memory.md). When the graphics device is lost (driver reset, GPU removed), Hammer Pro writes a report, saves the modified maps next to the autosaves and offers to restart with all your tabs opened again, see [Graphics device lost](graphics-device-lost.md).

### Choosing the graphics API

Hammer Pro renders with wgpu. By default it tries Vulkan, DirectX 12 (and Metal on macOS), preferring a GPU and API that can draw transparent windows (for the rounded window frame), then a dedicated GPU over an integrated one. The adapter chosen is written to the log file ("graphics adapter: ...").

To force an API, set the `WGPU_BACKEND` environment variable before starting Hammer Pro, for example in a command prompt:

```bat
set WGPU_BACKEND=vulkan
hammerpro.exe
```

Values: `vulkan`, `dx12`, `metal`, `gl` (several can be separated by commas). Try another value when the window stays black, flickers or the driver crashes at start.

Update your graphics driver first: most rendering problems come from old drivers.

## Window frame problems

Hammer Pro draws its own title bar and rounded frame. If the window cannot be moved or resized properly, is invisible over remote desktop, or your window manager has trouble with it, turn on **Settings > General > Native window frames**: the system's title bars and frames are used instead, at once. When the GPU cannot draw transparent windows, the frame is simply square.

The setting is stored as `native_frames = true` in the `[ui]` table of `%APPDATA%\HammerPro\settings.toml`, which you can edit by hand if the window is unusable.

## Starting from clean settings

If Hammer Pro misbehaves after a settings change, close it and rename `%APPDATA%\HammerPro\settings.toml`. The next start uses the defaults and opens the setup assistant. An invalid settings file is also ignored (defaults are used, a warning is logged).

## FAQ

**Hammer Pro closed and I lost my changes.** Open the map again: if an autosave or an emergency save is newer, Hammer Pro offers it. You can also look in `%LOCALAPPDATA%\HammerPro\autosave\` (files named `<map>_<hash>_<time>.vmf`).

**Where do I find what went wrong?** In the crash report (`%APPDATA%\HammerPro\crash\`) and the log file (`%LOCALAPPDATA%\HammerPro\logs\hammerpro.log`). Attach both to a bug report, see [Reporting bugs](reporting-bugs.md).

**The window is black or flickers.** Update the graphics driver, then try `WGPU_BACKEND=vulkan` or `WGPU_BACKEND=dx12`.

## Related pages

- [Out of memory](out-of-memory.md)
- [Graphics device lost](graphics-device-lost.md)
- [Autosave and crash recovery](../features/autosave-crash.md)
- [Window frames](../features/window-frames.md)
- [Reporting bugs](reporting-bugs.md)
- [Editor performance](performance.md)
- [Files and folders](../reference/files-and-folders.md)

---
[Back to the documentation index](../README.md)
