[Documentation](../README.md) > Troubleshooting > Discord presence not showing | [Français](../../fr/troubleshooting/discord-presence.md) | [Русский](../../ru/troubleshooting/discord-presence.md)

# Discord presence not showing

Hammer Pro can show what you are mapping in your Discord profile (the map, the active tool, the game and the time spent on the map), like Hammer++. When nothing appears, go through this checklist. The feature itself is described in [Discord Rich Presence](../features/discord.md).

## Checklist

1. **Read the status.** **Settings > Discord** shows a **Status:** line: *Connected*, *Connecting to Discord...*, *Discord not running*, *Discord refused the connection*, *Presence turned off*, *Invalid application ID*. The meaning of each is in [Discord Rich Presence](../features/discord.md#status). AI assistants read the same with the MCP tool `get_discord_status`.
2. **Show activity on Discord is on** (**Settings > Discord**, on by default). Turning it off clears the activity at once.
3. **The application ID.** Leave **Discord application ID** empty: Hammer Pro uses its own Discord application. A custom ID must be a number of 17 to 20 digits from the **General Information** page of your application; with a wrong one the status says *Discord refused the connection (Invalid Client ID (4000))*.
4. **The Discord desktop app runs on this computer.** Discord in a web browser cannot receive the presence: Hammer Pro talks to the local Discord client through its IPC pipes (`\\.\pipe\discord-ipc-0` to `discord-ipc-9` on Windows).
5. **Activity sharing is allowed in Discord.** In Discord's settings, **Activity Privacy**, turn on **Share your detected activities**.
6. **Wait a little.** Updates are sent at most every 15 seconds (a new map shows at once). When Discord is not running, Hammer Pro retries quietly, first after 5 seconds, then less often (up to every 20 seconds): starting Discord after Hammer Pro works, it takes up to 20 seconds.

Automated runs (`--script`, tests) never talk to Discord (status *Off in automated runs*).

## Using your own Discord application

Only to show another name or other pictures than Hammer Pro's:

1. Open the Discord Developer Portal and create an application (its name is what your profile shows).
2. Under **Rich Presence > Art Assets**, upload a large image with the key `hammerpro` (without it, the activity shows no picture).
3. On the **General Information** page, copy the **Application ID**.
4. In Hammer Pro, **Tools > Options...** (`Ctrl+,`), page **Discord**, paste it in **Discord application ID**. Empty the field to go back to Hammer Pro's application.

## The Discord settings page

| Option | What it does | Default |
|---|---|---|
| **Show activity on Discord** | Turns the presence on or off. | On |
| **Discord application ID** | Empty: Hammer Pro's own application. Else the ID of your own application. | Empty |
| **Show the map name** | Off: "Editing a map" instead of the map name. | On |
| **Show the active tool** | Off: the target game is shown instead (and "Compiling..." during a compile). | On |
| **Show the elapsed time** | The time spent on the map. | On |
| **"Get Hammer Pro" button link** | Optional web page (`https://...`) shown as a button under the activity; empty: no button. | Empty |

These are stored in the `[discord]` table of `settings.toml` (`enabled`, `application_id`, `show_map`, `show_tool`, `show_elapsed`, `project_url`).

## FAQ

**The activity shows but without the logo.** With Hammer Pro's application, wait a few minutes: Discord can be slow to send pictures. With your own application, the art asset key must be exactly `hammerpro`.

**Other people do not see the map name.** Check **Show the map name**. With it off, Discord shows "Editing a map".

**It worked, then stopped after restarting Discord.** Hammer Pro reconnects on its own within 20 seconds; the status shows *Discord not running* in the meantime.

## Related pages

- [Discord Rich Presence](../features/discord.md)
- [Settings window](../features/settings-dialog.md)
- [Settings reference](../reference/settings.md)
- [FAQ](faq.md)

---
[Back to the documentation index](../README.md)
