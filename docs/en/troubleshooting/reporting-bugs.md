[Documentation](../README.md) > Troubleshooting > Reporting bugs | [Français](../../fr/troubleshooting/reporting-bugs.md) | [Русский](../../ru/troubleshooting/reporting-bugs.md)

# Reporting bugs

Bug reports are very welcome. A good report lets the problem be reproduced quickly: this page lists what to include and where to find it.

## Where to report

**Help > Report an issue** opens the issue page of the project in your browser. Look first whether the problem is already reported; add your details to the existing report if it is.

## What to include

| Information | Where to find it |
|---|---|
| Hammer Pro version | **Help > About Hammer Pro** (version and build). |
| Windows version and graphics card | The log file names the graphics adapter and the API used ("graphics adapter: ..."). |
| What you did, what you expected, what happened | Short numbered steps. Mention the tool, the menu command or the key you used. |
| The map | The `.vmf` that shows the problem, if you can share it (or a small map made to reproduce it). For compile problems, the compile log too. |
| The crash report, after a crash | On the next start, the **Hammer Pro closed unexpectedly** window shows the report file: **Copy the report** puts its text on the clipboard to paste in your message, **Show the report file** opens its folder (`%APPDATA%\HammerPro\crash\`). |
| The log file | `%LOCALAPPDATA%\HammerPro\logs\hammerpro.log` (the current run) and `hammerpro.old.log` (the previous run). Errors of worker threads and of the graphics card are there and in the console panel. |
| Settings, when relevant | `%APPDATA%\HammerPro\settings.toml` (it holds paths of your computer; remove what you do not want to share). |

## Tips

- **Copy the console.** The console panel (`` ` ``) has a button that copies the visible messages: filter it to warnings and errors first.
- **Reproduce with defaults.** If you changed many settings, try again after renaming `settings.toml` (see [Crashes and logs](crashes-and-logs.md)). Say in the report whether the problem stays.
- **Graphics problems.** Say whether forcing another API with `WGPU_BACKEND` changes anything, and include a screenshot.
- **Compile problems.** Say which preset (Draft, Normal, Final) and target game you used, and whether the same map compiles with Valve's tools.
- **Slow editor.** Include the frame rate (**Settings > General > Show frame rate**) and the 3D view information (`O`) at the place where it is slow.
- **Translations.** A wrong or missing French text is a bug too: quote the text and where it appears.

## Contributing a fix

If you want to fix the bug yourself, see [Contributing](../development/contributing.md) and [Building from source](../development/building.md).

## Related pages

- [Crashes and logs](crashes-and-logs.md)
- [Common errors](common-errors.md)
- [FAQ](faq.md)
- [Contributing](../development/contributing.md)

---
[Back to the documentation index](../README.md)
