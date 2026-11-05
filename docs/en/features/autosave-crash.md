[Documentation](../README.md) > Features > Autosave and crash recovery | [Français](../../fr/features/autosave-crash.md) | [Русский](../../ru/features/autosave-crash.md)

# Autosave and crash recovery

Hammer Pro saves copies of your modified maps in the background, saves them again in an emergency when something goes wrong, offers to recover an autosave newer than the map you open, and writes a crash report you can send to the developers. A log file records every run.

## Autosave

Every few minutes, each open map with unsaved changes is written to a new autosave file, in the background (the console says **Autosaved**). The map file itself is never touched. The copy includes the cameras of the map.

Set it in **Settings > General**, section **Autosave**:

| Option | What it does | Default |
| --- | --- | --- |
| **Save every** | Minutes between autosaves, 0 to 120. 0 turns autosave off (**Disabled**). Key `edit.autosave_minutes`. | 5 min |
| **Copies kept per map** | Autosaves kept for each map, 1 to 100; the oldest are deleted. Key `edit.autosave_keep`. | 10 |

The page shows the folder: *Autosaves are stored in ...*. On Windows it is `%LOCALAPPDATA%\HammerPro\autosave`. Files are named `<map>_<hash>_<time>.vmf`, where the hash comes from the full path of the map: two maps with the same name in different folders never share their copies.

Autosaves are ordinary VMF files: open one with **File > Open map...** like any map.

## Recovering an autosave

When you open a map that has an autosave newer than the file (work you did not save), Hammer Pro asks **Recover autosave?** (*An autosave newer than map was found. Open it?*), with the autosave name:

| Button | What it does |
| --- | --- |
| **Open the autosave** | Opens the autosave instead (*Autosave opened. Save to keep it.*). |
| **Keep the saved map** | Opens the map file as it is. |
| **Open the autosave folder** | Shows the folder of the autosaves. |

If you closed a map with **Don't save**, its autosaves up to then are not offered: you threw that work away on purpose (the files stay until the rotation removes them).

## Emergency saves

- When an error stops a frame of the editor, the modified maps are saved next to the autosaves before the crash report is written, and the report records where.
- When the graphics device is lost (a driver reset, a GPU removed), the modified maps of every tab are saved next to the autosaves and Hammer Pro offers to restart: **Yes** starts it again with all your tabs, the modified maps opened from their saved changes. See [Graphics device lost](../troubleshooting/graphics-device-lost.md).
- When the computer runs out of memory, Hammer Pro uses a small reserve to go on, saves the modified maps at once and asks you to free memory. See [Out of memory](../troubleshooting/out-of-memory.md).
- A graphics error that would only spoil one frame is shown in the console (*Graphics error (one frame was skipped)*) and Hammer Pro keeps running; more of them go to the log file.

## Crash reports

When Hammer Pro crashes, it writes a report (message, location, thread, backtrace and the last lines of the log) to `crash-<date>.txt` in the crash folder, `%APPDATA%\HammerPro\crash` on Windows. The 10 newest reports are kept.

On the next start, a window **Hammer Pro closed unexpectedly** shows it once:

| Control | What it does |
| --- | --- |
| **Report file:** | Where the report is. |
| **Show the report file** | Opens its folder with the report selected. |
| **Copy the report** | Copies its text to the clipboard, to paste it in a message. |
| **Open the saved changes** | Opens the emergency save, when there is one (*Your unsaved changes were saved to ...*). |

A crash outside Rust code (an access violation in the graphics driver, a stack overflow) or an allocation that cannot be served writes a report too, with the module where it happened and the last lines of the log; when it happens in a background thread, the modified maps are saved first. Crashes that leave no chance to write anything (the process killed) are caught too: each run leaves a marker that a normal close removes, and the next start turns a marker left by a dead process into a report with the end of the previous run's log.

Errors in background threads do not close Hammer Pro: they are logged, and the first few of a run also get a report file (not offered at the next start) whose path is in the log.

Runs started with `--script` or `--screenshot` never read or write the crash folder.

## Log file

Each run writes `hammerpro.log` line by line in the logs folder, `%LOCALAPPDATA%\HammerPro\logs` on Windows. The previous run is kept as `hammerpro.old.log`. Warnings and errors also show in the console of the editor. Attach the log to a bug report (see [Reporting bugs](../troubleshooting/reporting-bugs.md)).

## Tips

- Lower **Save every** to 1 or 2 minutes on a big session if your machine is fast; the save runs in the background.
- After a crash, click **Copy the report** and paste it in your bug report: it has everything the developers need.
- Use **Map > Compare with another map...** on an autosave to see what changed before you recover it.

## FAQ

**Does autosave overwrite my map?** No. Autosaves go to their own folder; only **Save** writes your map file.

**Are untitled maps autosaved?** Yes, every open map with unsaved changes is autosaved.

**I turned autosave off and lost work.** Set **Save every** back above 0; emergency saves still happen on a crash, but regular autosaves are your safety net.

## Related pages

- [Crashes and logs](../troubleshooting/crashes-and-logs.md)
- [Reporting bugs](../troubleshooting/reporting-bugs.md)
- [Files and folders](../reference/files-and-folders.md)
- [Settings window](settings-dialog.md)

---
[Back to the documentation index](../README.md)
