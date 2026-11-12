[Documentation](../README.md) > Guides > Use Hammer Pro with an AI assistant | [Français](../../fr/guides/mcp-with-ai-assistant.md) | [Русский](../../ru/guides/mcp-with-ai-assistant.md)

# Use Hammer Pro with an AI assistant

Hammer Pro's built-in MCP server lets an AI assistant (Claude Code, Claude Desktop, Codex, Gemini CLI or any MCP app) read and edit the map open in a Hammer Pro window: build rooms, place entities, fix problems, compile and look at screenshots of the views to check its work. This guide sets it up and shows how to work with it. Setup details and security are in [MCP server](../reference/mcp-server.md).

## Step 1: switch the server on

At the right end of the top bar, click the **MCP off** pill. It becomes **MCP on · port · clients**: the server of this window is running. It is off by default in every window; to have it on at every start, tick **Start the MCP server when Hammer Pro starts** on the **MCP** page of the settings (**Tools > Options...**).

## Step 2: install Hammer Pro in your AI app

1. Click the small arrow next to the pill (or **Tools > Blender and MCP > MCP...**) to open the MCP window.
2. Click **Install in Claude, Codex, Gemini**. Hammer Pro adds itself to every supported app it finds, then checks each one. Each app line ends on **Installed and configured** when it worked.
3. Restart the AI app, or start a new session, so it sees the `hammer-pro` server.

You only do this once per computer (again only if you move Hammer Pro to another folder: click **Update**). For an app that is not in the list, use the **Manual setup** section: a stdio server with the command `hammerpro.exe` and the argument `--mcp`.

## Step 3: ask for something

Open your map in Hammer Pro, make sure the pill shows **MCP on**, then talk to the assistant in plain words. For example:

- "Look at my Hammer Pro map and describe it."
- "Add a 512 x 512 floor with `dev/dev_measuregeneric01b`, four walls 128 high and a light in the middle."
- "Place an `info_player_start` in the room and check the map for problems."
- "Make the stairs of the north building out of 8 unit steps."
- "Find every face using a missing material and replace it with `concrete/concretefloor001a`."
- "Compile with the Draft preset and tell me if it leaks."

The assistant starts with `get_capabilities` and `get_map_info`, uses the tools it needs, and can take screenshots of the views (`screenshot_view`) or of the whole window (`screenshot_window`) to check the result.

## Step 4: review and undo

- Every change the assistant makes is **one undo step** named "MCP:" and what was done (for example **MCP: apply material**). `Ctrl+Z` takes it back like your own edits; the undo history shows them all.
- The map is not saved unless you, or the assistant on your request, save it. Autosave keeps working as usual.
- Watch the views while it works: tool calls run between two frames of the editor, like your clicks.

## What the assistant may and may not do

- It only reaches windows whose pill is **MCP on**, and only from this computer (local address, secret token).
- It runs no program on its own and only opens or saves `.vmf` maps. Replacing another existing file needs an explicit overwrite. It can make or change a compiler that runs other programs only with a confirmation, when you asked for it.
- Sensitive commands (exit, switching MCP off, installing MCP in other apps, publishing, starting the game, file dialogs...) need a confirmation that the assistant should pass only when you asked for exactly that.
- It cannot change file paths, programs, command lines, game content folders or the MCP settings: you do that in the Settings window.

## Several Hammer Pro windows

With several windows, the assistant uses the one with MCP on that you focused last. Ask it to "list the Hammer Pro windows" and "use the one with gm_city" (tools `list_instances` and `use_instance`). Inside one window, it can also switch between the open maps (`list_tabs`, `switch_tab`).

## Placing props and objects precisely

The assistant does not have to guess sizes from bounding boxes. Ask it to place things and it uses the placement tools, which measure the real geometry of everything: the model mesh after its angles and scale, brushes, displacements, overlays, instances and groups.

- "Put a couch against the north wall of the living room, on the floor." The assistant measures the couch (`get_object_shape`: real size, how high the origin sits above the feet), then places it flush against the wall and on the floor (`place_object` with `against`, then `drop`).
- "Put a barrel on the table." It uses `place_object` with `on`: the barrel's lowest point lands exactly on the table top, no gap, no overlap.
- "Park the car on the hill." The car settles on the real displaced surface, not on the brush under it.
- "Find everything that floats or is sunk in this street and fix it." `find_floating_objects` checks every prop on its real mesh, keeps wall lamps and signs that touch a wall, and drops or lifts the others in one undo step.

`render_shape` gives the assistant a drawing of a model or an area from several sides with the sizes written on it, and `check_placement` tells it, for each object, whether it rests, floats, is sunk or goes into another object. The full guide is in [docs/MCP.md](../../MCP.md#place-things-precisely).

## Optimizing what the assistant built

Ask the assistant to finish with `optimize_map`: it compiles the visibility in the background, lists what is not optimized (visleaves, func_detail, hints, areaportals, lights) and, with `apply: "safe"`, fixes the safe part in one undo step and reports the visleaves and the PVS before and after. See [Optimizing a map](optimizing-a-map.md).

## Tips

- Be concrete: sizes in units, material names, entity classes. The assistant can also search them (`search_materials`, `search_models`, `list_entity_classes`).
- Ask it to check its work: "take a screenshot of the 3D view", "run the problem check".
- For big changes, save first, or ask for one step at a time so each one is easy to undo.
- The assistant can drive every menu command and setting too (`run_action`, `set_setting`), and even the interface itself (`describe_ui`, `ui_click`...) for what has no dedicated tool.

## FAQ

**The assistant says no Hammer Pro window has MCP on.** Click the pill in the Hammer Pro window you want to use.

**The app does not list the hammer-pro tools.** Restart the app or open a new session after installing. In the MCP window, click **Verify** on its line.

**Can it compile and start the game?** It compiles with the compiler chosen for the game (Hammer Pro's own by default) when you ask, and can choose or set up another one (Valve's tools, your own) when you ask. It starts the game only when you explicitly ask for it, and never in automated runs.

## Related pages

- [MCP server](../reference/mcp-server.md)
- [MCP tool reference](../../MCP.md)
- [Undo and history](../features/undo-history.md)
- [Check for problems](../features/check-problems.md)
- [Compile and test in Garry's Mod](compile-and-test-gmod.md)

---
[Back to the documentation index](../README.md)
