[Documentation](../README.md) > Features > Updates | [Français](../../fr/features/updates.md) | [Русский](../../ru/features/updates.md)

# Updates

Hammer Pro looks for a newer release on GitHub when it starts, tells you what is new, and installs it for you: it downloads the release, closes, replaces `hammerpro.exe` where it is installed, and starts again. Nothing is downloaded before you click **Update**.

## What happens at startup

1. Right after the window opens, a background thread asks the GitHub releases API of the Hammer Pro project (`oracleidt/hammerpro`, latest release) for the newest tag, for example `v1.0.3`.
2. The tag is compared with the version you run, number by number (`1.0.10` is newer than `1.0.9`).
3. When the release is newer, the **Update available** window opens. Otherwise nothing is shown.

The check never blocks the editor and never shows an error: without a network connection, behind a firewall, or when GitHub is slow, the editor simply starts as usual. Automated runs (`--script`, `--screenshot`, `--bench-open`) never check.

## The Update available window

| Part | What it shows |
| --- | --- |
| Title line | *Hammer Pro 1.0.3 is available* and *You are using version 1.0.2.* |
| **What's new** | The notes of the release (its description on GitHub), scrollable. Headings, bullet lists and plain paragraphs are formatted. |
| **Update** | Downloads the release archive and installs it (see below). |
| **Ignore** | Closes the window. With **Don't show this again** ticked, this version is skipped: the window does not open at startup for it any more, and comes back for the next release. |
| **Don't show this again** | Remembers the version in the setting `update.skipped_version`. A manual check (**Check for updates...**) still shows it. |

When a release has no Windows archive, the window offers **Open the release page** instead of **Update**.

## Installing

1. **Update** downloads `HammerPro-<version>-windows.zip` to a temporary folder, with a progress bar and a **Cancel** button. The size is checked and the archive must contain `hammerpro.exe`, so a bad download is refused before anything is closed.
2. Hammer Pro then closes like **File > Exit**: if a map has unsaved changes, you are asked to save, discard or cancel first. **Cancel** keeps the session open and the downloaded update waiting (the window offers **Restart and install**). The update does not start while a compile is running.
3. A small helper (a copy of `hammerpro.exe` run from the temporary folder, without a window) waits for the program to end, and closes any process of `hammerpro.exe` that still holds the file. It extracts the new `hammerpro.exe` next to the old one, renames the old one to `hammerpro.exe.old` (kept as a backup), puts the new one in its place, retrying while Windows keeps the file locked, then starts the new `hammerpro.exe`.
4. If anything fails (the file stays locked, the disk is full, the archive is damaged), the old program is put back, an error window explains what happened, and the old Hammer Pro starts again. Your settings are not touched.

The program is replaced **at the same place** as the one you ran, so a copy installed anywhere (Desktop, `D:\Tools\HammerPro`...) is updated in place. If that folder needs administrator rights (for example `C:\Program Files`), the update fails safely and says so; run Hammer Pro from a folder you can write to.

## Settings > Updates

**Tools > Options...** (`Ctrl+,`), page **Updates**:

| Option | What it does | Default |
| --- | --- | --- |
| **Check for updates on startup** | Turns the background check at startup off or on. Key `update.check_on_startup`. | on |
| **Show the update window when a new version is out** | The master switch of the window. Turn it back on to undo **Don't show this again**, whichever version you hid: it clears the skipped version too. Key `update.show_dialog`. | on |
| **Check for updates now** | Checks at once and opens the window when a newer release exists (or tells you that you are up to date). | |
| **Show the update** | Reopens the window of the update found. | |
| Status line | *Hammer Pro is up to date (latest release: 1.0.2)*, *Version 1.0.3 is available.*, or the reason the last check failed. | |
| **Skipped version** and **Stop skipping** | Shown when you ticked **Don't show this again**. Key `update.skipped_version`. | empty |

**Help > Check for updates...** (and the command palette) does the same as **Check for updates now**.

## For testing: the HP_UPDATE variables

Two environment variables let you try the whole flow without a real release:

| Variable | Effect |
| --- | --- |
| `HP_UPDATE_API_URL` | Replaces the GitHub address. Point it to a local HTTP server answering with the JSON of a release (`tag_name`, `body`, `assets` with `name`, `browser_download_url` and `size`). Downloads are then accepted from that same server. The Updates page says that the address is replaced. Automated runs only check when it is set. |
| `HP_UPDATE_CURRENT_VERSION` | Pretends this version is the installed one (for example `1.0.0`), so any release looks newer. |

Without `HP_UPDATE_API_URL`, downloads are only accepted from `github.com` (HTTPS) and the GitHub download hosts.

## FAQ

**Is anything sent to GitHub besides the request?** No. It is one anonymous request for the latest release, with a `User-Agent` of `HammerPro/<version>`. Nothing about you or your maps is sent.

**Where is the download kept?** In `%TEMP%\HammerPro-update-<process>-<tag>`. Old folders are removed at a later start.

**How do I go back?** Close Hammer Pro, delete `hammerpro.exe` and rename `hammerpro.exe.old` to `hammerpro.exe`. Skip the version with **Don't show this again** so it is not offered again.

**I do not want any update message.** Untick **Check for updates on startup**. You can still check by hand.

**The update failed.** Read the message; the log of the helper is `update.log` in the temporary folder `%TEMP%\HammerPro-updater-<process>`. The most common causes are an antivirus holding the file and a folder without write access.

## MCP

AI assistants read the state with `get_update_status` (installed version, status, the newest release with its notes, progress, errors, preferences), ask for a check with `check_for_updates`, install with `apply_update` (needs `confirm: true`, an update found, and no compile running; it closes Hammer Pro, so only on the user's request), and change `update.check_on_startup`, `update.show_dialog` and `update.skipped_version` with `set_setting`. The action `install_update` of `run_action` needs `confirm` too. See the [MCP tool reference](../../MCP.md).

## Related pages

- [Settings window](settings-dialog.md)
- [Settings reference](../reference/settings.md)
- [Command line](../reference/command-line.md)
- [Files and folders](../reference/files-and-folders.md)

---
[Back to the documentation index](../README.md)
