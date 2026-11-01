[Documentation](../README.md) > Features > Discord Rich Presence | [Français](../../fr/features/discord.md)

# Discord Rich Presence

Like Hammer++, Hammer Pro can show what you are mapping in your Discord profile: the map, the active tool, the game and the time spent on the map.

## Opening it

**Settings > Discord** (**Tools > Options...**, `Ctrl+,`), section **Discord Rich Presence**. Settings are stored as `[discord]` in settings.toml.

## Options

| Option | What it does | Default |
| --- | --- | --- |
| **Show activity on Discord** | Turns the presence on. Turning it off clears the activity at once. Key `enabled`. | on |
| **Show the map name** | Shows *Editing gm_city.vmf*; off, *Editing a map*. Key `show_map`. | on |
| **Show the elapsed time** | Shows the time spent on the map. Key `show_elapsed`. | on |
| **Show the active tool** | Shows the tool you use. Off: the target game is shown instead (and *Compiling...* during a compile). Key `show_tool`. | on |
| **Discord application ID** | Optional: the ID of your own Discord application, to replace Hammer Pro's (section **Discord application**). Empty: Hammer Pro's own application is used. Key `application_id`. | empty (Hammer Pro's application) |
| **"Get Hammer Pro" button link** | Optional web page (`https://...`) shown as a **Get Hammer Pro** button under the activity; empty: no button. Key `project_url`. | empty |

## What Discord shows

| Line | Content |
| --- | --- |
| Details | *Editing map.vmf*, *Editing a map* (map name hidden) or *No map open*. |
| State | The active tool, or *Compiling...* during a compile, or the name of the game. |
| Time | Time since the map was opened (when **Show the elapsed time** is on). |
| Large image | The Hammer Pro logo (image key `hammerpro`), with "Hammer Pro" and the version. |
| Small image | The icon of the game of the map, with its name. |

## The application

Hammer Pro comes with its own Discord application, named "Hammer Pro", with the logo under the image key `hammerpro`: there is nothing to set up. The **Discord application ID** field is empty and shows *Hammer Pro (built in)*.

To show the activity under another name or with other pictures, create your own application on the Discord Developer Portal, upload your images under **Rich Presence > Art Assets** (key `hammerpro` for the large image; game icons with the keys `gmod`, `hl2`, `hls`, `hl2dm`, `tf2`, `css`, `dods`, `portal`, `portal2`, `l4d`, `l4d2`, `csgo`, other games use their own game id), and paste its **Application ID** (General Information page) in the field. The ID is a number of 17 to 20 digits; anything else is refused with a message. Empty the field to go back to Hammer Pro's application.

## Status

The line **Status:** under the options says what the presence is doing, and updates within a second:

| Status | Meaning |
| --- | --- |
| *Connected: your activity is shown on Discord* | Hammer Pro talks to the Discord app and the activity is sent. |
| *Connecting to Discord...* | First connection, or an option just changed. |
| *Discord not running (retrying in the background)* | No Discord desktop app answered on this computer. Hammer Pro tries again after 5 seconds, then less often (every 20 seconds at most): start Discord and the activity shows up by itself. |
| *Discord refused the connection (reason)* | Discord answered but refused the application, for example *Invalid Client ID (4000)* with a wrong custom ID. |
| *Presence turned off* | **Show activity on Discord** is off. |
| *Invalid application ID: presence disabled* | The field holds something that is not an application ID. |
| *No application ID: presence disabled* | No ID to use (only in builds without the built in application). |
| *Off in automated runs* | `--script`, tests: the presence never starts. |

## Requirements

The Discord desktop app must run on the same computer, with **Share your detected activities** on in Discord's **Activity Privacy** settings. Hammer Pro talks to it directly through the local Discord connection, on a background thread: when Discord is not running, it retries quietly and the editor never waits for it. Automated runs (`--script`, tests) never reach your Discord.

## Tips

- Turn off **Show the map name** when you work on a secret project; the presence then just says *Editing a map*.
- Share a link to your map's Workshop page or your community with the button link.

## FAQ

**Nothing shows on Discord.** Look at the **Status:** line in **Settings > Discord**, check that the Discord desktop app runs and that activity sharing is on. See [Discord presence not showing](../troubleshooting/discord-presence.md).

**Does the presence slow down the editor?** No, it runs on its own thread and updates are throttled.

**Does it work with the Discord web app?** No, the presence needs the desktop app on the same computer.

**Do I need to create a Discord application?** No, Hammer Pro has its own. A custom ID is only for showing another name or other pictures.

## MCP

AI assistants read the state with `get_discord_status` (status, its text, the application ID in use, the details and state lines sent) and change the options with `set_setting` on the `discord.*` keys. See the [MCP tool reference](../../MCP.md).

## Related pages

- [Discord presence not showing](../troubleshooting/discord-presence.md)
- [Settings window](settings-dialog.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
