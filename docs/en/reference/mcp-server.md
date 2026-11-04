[Documentation](../README.md) > Reference > MCP server | [Français](../../fr/reference/mcp-server.md)

# MCP server

Hammer Pro has an MCP server built in. MCP (Model Context Protocol) is how AI apps such as Claude Code, Claude Desktop, Codex and Gemini CLI use tools: with it, an AI assistant can read the map open in Hammer Pro, edit it, check it, compile it and take screenshots of the views to check its own work. This page covers setup and security. The complete list of the tools with their arguments is the [MCP tool reference](../../MCP.md).

## How it works

```text
AI app (Claude, Codex, Gemini)
    | starts "hammerpro.exe --mcp" and talks MCP on its stdin and stdout
hammerpro --mcp                     the bridge: no window
    | 127.0.0.1, random port, secret token
Hammer Pro window with MCP on       runs the tools on the map it shows
```

- Each Hammer Pro window has **its own server, off by default**.
- The AI app starts `hammerpro.exe --mcp` itself once it is installed. That process opens no window: it forwards the tool calls to a Hammer Pro window whose server is on.
- When no window has MCP on, the AI still sees the tools, and each tool answers that you must open Hammer Pro and switch MCP on.
- Tool calls run on the editor's own thread, between two frames, like one of your clicks. The bridge waits up to 150 seconds for an answer.
- The bridge speaks the MCP protocol versions 2025-11-25, 2025-06-18, 2025-03-26 and 2024-11-05 (JSON-RPC 2.0 over stdio).

## The MCP pill

At the right end of the top bar, the pill shows the state of the server of this window:

| Pill | Meaning |
|---|---|
| **MCP off** | The server of this window is off. Nothing listens. |
| **MCP on · port · clients** | The server is on, with its local port and the number of connected AI apps (**1 client**, **2 clients**...). |

- **Click the pill** to switch the server on or off. Its tooltip also shows the last tool called and the number of calls.
- **The small arrow** next to the pill (or a right click on the pill, **Install in AI apps, setup and tools**) opens the MCP window.
- The same commands are in the **Tools** menu: **MCP server** (on/off), **Install MCP in Claude, Codex and Gemini** and **MCP...** (the MCP window).

To have the server on when Hammer Pro starts, tick **Start the MCP server when Hammer Pro starts** on the **MCP** page of the Settings window (**Tools > Options...**, `Ctrl+,`). Off by default (setting `mcp.start_with_app`). The same page has a **Switch on**/**Switch off** button, **Install and setup...** (opens the MCP window) and the list **Tools exposed to AI apps**.

## The MCP window (Hammer Pro MCP)

| Section | What it contains |
|---|---|
| **Server of this window** | The state (**Off**, or **On, local port N, clients**) and a **Switch on**/**Switch off** button. |
| **AI apps** | The **Install in Claude, Codex, Gemini** button, then one line per app with its state and the **Install** (or **Update**), **Verify** and **Remove** buttons. |
| **Manual setup** | A folding section with ready to paste snippets using the real path of your `hammerpro.exe`, each with a **Copy** button. |
| **MCP documentation** | Link to the full tool reference. |

States of an app line: **Not checked yet: click Verify**, **Not installed on this computer**, **Installed, not configured**, **Installed and configured**, **Configured for another copy of Hammer Pro (...): click Update**, or **Error: ...**.

The apps' own programs (the `claude` command) only run when you click **Install**, **Verify** or **Remove**. Opening the window only reads their configuration files.

## Install in an AI app

1. Switch the pill on (**MCP off** becomes **MCP on**).
2. Open the MCP window (arrow next to the pill, or **Tools > MCP...**).
3. Click **Install in Claude, Codex, Gemini**. Hammer Pro adds itself to every app it finds on this computer, in this order: Claude Code, Claude Desktop, Codex, Gemini CLI, then checks each one. You can also install a single app with its own **Install** button.
4. Restart the AI app (or start a new session) so it sees the new server.

What each install changes:

| App | What is changed |
|---|---|
| Claude Code | Runs `claude mcp add --scope user hammer-pro -- "<hammerpro.exe>" --mcp`, checked with `claude mcp get hammer-pro`. `~/.claude.json` is never edited directly. |
| Claude Desktop | `mcpServers` of `%APPDATA%\Claude\claude_desktop_config.json` (or of the Microsoft Store copy under `%LOCALAPPDATA%\Packages\Claude_*`). |
| Codex | `[mcp_servers.hammer-pro]` of `~/.codex/config.toml` (or of `%CODEX_HOME%`). |
| Gemini CLI | `mcpServers` of `~/.gemini/settings.json`. |

Before the first change, each file is copied to `<file>.hammerpro.bak`. Files are written through a temporary file, everything else in them is kept (other servers, comments in the TOML file) and the result is read back to verify it. A JSON file with comments is not touched: add the entry by hand.

Moved Hammer Pro to another folder? Open the MCP window: the apps show **Configured for another copy of Hammer Pro**. Click **Update** (or **Install in Claude, Codex, Gemini**).

## Manual setup

If you prefer, or if the automatic install cannot edit a file, add the server yourself (the MCP window shows these snippets with your real path):

Claude Code:

```bash
claude mcp add --scope user hammer-pro -- "C:\Program Files\Hammer Pro\hammerpro.exe" --mcp
```

Claude Desktop (`claude_desktop_config.json`) and Gemini CLI (`~/.gemini/settings.json`):

```json
{
  "mcpServers": {
    "hammer-pro": {
      "command": "C:\\Program Files\\Hammer Pro\\hammerpro.exe",
      "args": ["--mcp"]
    }
  }
}
```

Codex (`~/.codex/config.toml`):

```toml
[mcp_servers.hammer-pro]
command = 'C:\Program Files\Hammer Pro\hammerpro.exe'
args = ["--mcp"]
```

Any other MCP app: a stdio server with the command `hammerpro.exe` and the argument `--mcp`.

## Security

- **Off by default**, per window. Nothing listens until you switch the pill on.
- **Local only**: the server listens on `127.0.0.1`, never on the network, and refuses any connection that is not from this computer.
- **Secret token**: each time a server starts it makes a random 32 byte token. Every connection must send it first or is closed. The token is only written in the window's instance record, `%LOCALAPPDATA%\HammerPro\mcp\instances\<pid>.json`, in your own profile. The record is removed when the server stops or Hammer Pro closes; records of crashed windows are ignored and cleaned up.
- **Limits**: messages up to 8 MB, 8 connections at a time per window, a timeout on every tool call.
- **No programs unless you ask, no arbitrary files**: the tools run no command of their own. Maps are opened and saved as `.vmf` only; saving over another existing file needs an explicit `overwrite`. The game is started only when the AI explicitly asks for it. Compiles use the compiler chosen for the game (Hammer Pro's own by default); creating or changing a compiler that runs other programs (`create_compiler`, `set_compiler` with steps) needs `confirm: true`. See [Choosing a compiler](../compiler/compilers.md).
- **Confirmation**: `run_action` runs any menu command except a few that need `confirm: true`, which the AI passes only when you asked for exactly that: Exit, switching MCP off, installing MCP in other apps, publishing, starting or talking to the game, compiling with the game, editing an instance in another window, opening the web browser and the commands that show a file dialog.
- **Settings**: `set_setting` never changes file paths, folders, programs, command lines, the game content folders or the MCP settings: you change those in the Settings window. Adding or removing an extra FGD file (`.fgd` only) needs `confirm: true`.
- The UI automation tools (`ui_click`, `ui_key`, `ui_type`...) act inside the Hammer Pro window only.
- Automated runs (`--script`) never start the game, confirmed or not.

## Several windows

With several Hammer Pro windows, the bridge uses the one with MCP on that you focused last, and stays with it while it runs. The AI can list the windows with `list_instances` and switch with `use_instance`. To pin one window in an app's configuration, set the `HAMMERPRO_INSTANCE` environment variable to its process id.

## Undo

Every change the AI makes is **one undo step** named "MCP:" followed by what was done (for example **MCP: apply material**), in the **Edit** menu and the undo history. `Ctrl+Z` takes it back like any of your own edits. The AI can also undo and redo (`undo`, `redo`, `undo_steps`, `redo_steps`) and read the history (`get_history`).

## Tool categories

| Category | What the tools cover |
|---|---|
| Help | `get_capabilities`: every tool by category with an example call. The AI starts here. |
| Map | `get_map_info`: file, unsaved changes, counts, bounds, selection, undo steps, content and compile state. |
| Files | Open a `.vmf`, save the map. |
| Brushes and objects | List solids, create brushes (block, wedge, cylinder, spike, sphere, arch, torus, stairs, spiral stairs, ramp, pillar, doorway), apply materials, move, rotate, delete. |
| Entities | List, read, create entities and set their keyvalues. |
| Selection | Read, change and clear the selection. |
| History | Undo and redo. |
| Clipboard | Read the VMF text of objects and paste VMF text. |
| Assets | Search materials, models, sounds and particles, material and model information and previews, VMT editing, image import, UV layers and texture alignment. |
| Map health | Check for problems, fix them, engine limits, map statistics, hidden faces. |
| Compile | Compile with a preset and follow the progress. |
| Views | Camera, screenshots of a view, layout of the quadrants, 3D render modes, player view, 2D zoom and pan. |
| Actions | List and run every menu command and shortcut. |
| Settings | Read and change settings and key bindings. |
| UI automation | Screenshot of a window, description of the interface, mouse and keyboard input inside the window. |
| Windows | List the Hammer Pro windows and choose one. |
| Editing | Duplicate, array, transform, resize, flip, snap, align, clip, carve, face outlines carved or extruded, edge bevels (rounds, chamfers, coves, inner corner fills), hollow, merge, vertex editing, primitives, rooms, stairs, hint and nodraw, tie to entity, face textures (justify, fit, wrap), material replacement. |
| Groups and visibility | Select by class, name, material or box, groups, visgroups, hide and isolate, cordons, display colors. |
| Grids | Grid modes, custom grids (create, edit, delete, attach). |
| Displacements | Create, destroy, paint, sculpt strokes, noise, scale, alpha, flags, tags, subdivide, sew. |
| Instances and prefabs | func_instance, instance parameters, collapse, prefabs, sprinkle, ray tests. |
| Entities and IO | Entity classes, keyvalues, spawnflags, outputs, I/O graph, find and replace text, entity report, targetnames. |
| Compile and play | Every compile option, compile log and statistics, history, leaks, pointfiles, portals, baked lighting preview, launching the game. |
| Content and release | Dependency report, Build and save as content, checklist, Workshop publishing, map assets collection, unused files. |
| Documents | Tabs of open maps, new map, recent maps, import and export, BSP Decompiler, nav mesh, map properties. |
| BSP Sniffer | Start, cancel and follow a [BSP Sniffer](../features/bsp-sniffer.md) run (status, log, report) and read or change every setting. Needs the setting `authorized`, which only the user should turn on. |
| History and clipboard | Undo history, several undo or redo steps, copy, cut, paste, paste special. |
| Overlays and decals | List and create overlays and decals. |
| Paths | Path entity chains. |
| Roads | Create, read, list, edit, delete and split [roads](../features/road-tool.md), and join the end of a road to the side of another with flares. |
| Skybox | Sky cameras, preview, skybox switch. |
| Entity definitions | FGD information, add, remove, reload, validate FGD files. |
| Games | List the supported games, read and change the game of the map. |
| Tool options | Read and change the options of the editing tools. |

The full list of every tool with its description and arguments is in [docs/MCP.md](../../MCP.md) (English).

## Related pages

- [Use Hammer Pro with an AI assistant](../guides/mcp-with-ai-assistant.md)
- [MCP tool reference](../../MCP.md)
- [Command line](command-line.md)
- [Script commands](script-commands.md)
- [Settings reference](settings.md)
- [Undo and history](../features/undo-history.md)

---
[Back to the documentation index](../README.md)
