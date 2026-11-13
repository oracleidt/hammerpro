[Documentation](../README.md) > Features > Game launch | [Français](../../fr/features/game-launch.md) | [Русский](../../ru/features/game-launch.md)

# Game launch

Hammer Pro tests your map in game quickly: it loads the compiled map in the Garry's Mod already running instead of starting the game again, can boot the game while the map compiles, can place you at the editor camera, and can bring props you arranged in game back into the map. Other Source games are started on the map too, without the Garry's Mod extras.

## Commands

| Command | Default key | What it does |
| --- | --- | --- |
| **File > Compile > Compile and run...** | `Ctrl+F9` | Opens the compile window, where **Launch the game** runs the map after the compile. |
| **File > Compile > Compile now** | `F9` | Compiles with the current settings (and launches the game when **Launch the game** is on). |
| **File > Compile > Run the map in game** | | Loads the compiled map of the open document in the running game, or starts the game on it. |
| **File > Compile > Reload the map in game** | | The same, for a game already running. |
| **File > Import > Import props from the game** | | Brings the props placed in game into the map (needs the bridge). |

With the Hammer or Hammer++ shortcut set, `F9` opens the compile window and `Ctrl+F9` compiles at once (see [Keyboard shortcuts](keyboard-shortcuts.md)).

The map must be saved and compiled: the compiled map takes the name of the VMF (*Save and compile the map first*). For Garry's Mod, `<map>.bsp` must be in the maps folder of the game (*compile the map first*).

## How the map gets into the game

- **Reuse the running game** (on by default): when Garry's Mod is running, the map is loaded in it, without a second process and without the "Source Engine Not Running" box while the game boots. Otherwise the game starts on the map.
- If the game is starting, the map waits for the game window and loads as soon as it is ready (*Garry's Mod is starting: the map loads as soon as it is ready*).
- The status bar shows the state of the game: **Game ready**, **Game starting**, or nothing when it is not running.

## Launch options of the compile window

Under the compile settings, the **Game** row holds the launch options. They apply when **Launch the game** is ticked and the game is not opened **Through Steam**.

| Option | What it does | Default |
| --- | --- | --- |
| **Launch the game** | Runs the map after the compile. | on |
| **Through Steam** | Opens the map with a `steam://` link instead of starting the game executable. | off |
| **Game arguments** | Extra arguments for the game. `-game`, `-hijack` (reuses a running game) and `+map` are added unless you set them. `{map}` is replaced by the map name. | `-console -dev` (plus `-novid`, added once) |
| **Start the game:** **after the compile** | Starts the game once the map is compiled (like Hammer). | selected |
| **Start the game:** **when the compile starts** | Starts the game as soon as the compile starts: it boots while the compiler runs, then loads the map. Garry's Mod only. | |
| **Reuse the running game** | Loads the map in the game already running instead of starting it again. | on |
| Game state | **Game ready**, **Game starting** or **Game not running**. | |

### Fast start

The **Fast start** button opens options that make the game start faster for a test. Each option adds or removes its flag in **Game arguments**, so what you read in that field is what runs.

| Option | Flag | What it does |
| --- | --- | --- |
| **Skip the intro video** | `-novid` | Saves a few seconds at every start. Added once to the game arguments at the first start. |
| **Windowed** | `-windowed` | Runs the game in a window. |
| **Borderless** | `-noborder` | Window without borders. |
| **Window size** | `-w`, `-h` | Width and height of the game window (1600 x 900 when turned on; 320 to 7680 by 240 to 4320). |
| **Open the console** | `-console` | Shows the developer console at start. |
| **Developer messages** | `-dev` | Shows developer messages; also skips the menu background map. |
| **Private local game** | `+sv_lan 1 +maxplayers 1` | A local server for you alone: no network setup, faster to start. |
| **Skip unused Workshop and addons** | `-noworkshop`, `-noaddons` | When the map uses nothing from them, the game does not mount Workshop downloads and addons: faster start, but addon tools and weapons are not available in game. Setting `launch.skip_unused_content`, off by default. |
| **Report the map load time** | | Tells how long the game took to load the map and where the time went (files and content, entities and scripts). Needs the bridge. Setting `launch.load_report`, off by default. The last result shows below: *Last load: map in N s*. |

## The Hammer Pro bridge

The bridge is a small addon in `garrysmod/addons/hammerpro_bridge` that lets Hammer Pro place you at the editor camera and import the props you arrange in game. It is never packed into your maps, and it only acts in local games (listen servers).

Install it from **Settings > Game**, section **Testing in game**:

| Control | What it does |
| --- | --- |
| **Hammer Pro bridge: installed / not installed / no game folder** | State of the addon. |
| **Install** / **Update** / **Remove** | Installs, updates or removes the addon. Restart the game afterwards. |
| **Spawn at the editor camera** | When the map loads in game, places the player where the 3D camera of the editor is (noclip). Needs the bridge. Setting `launch.spawn_at_camera`, off by default. |

The editor and the game exchange small files in `garrysmod/data/hammerpro/` (`cmd.json`, `ack.json`, `props.json`); everything read back is size limited and checked.

### Import props from the game

Spawn and arrange props in game (with the bridge installed and the map running), then run **File > Import > Import props from the game**. The props come into the map as one undo step: `prop_physics` stays `prop_physics`, `prop_dynamic` and `prop_physics_multiplayer` become `prop_static`, with their model, skin, position and angles. Without the bridge you get *Install the Hammer Pro bridge first (Settings > Game), then restart the game*; without an answer, *The game did not answer: is a map running with the bridge installed?*

## Other games

For a game other than Garry's Mod, **Run the map in game** starts its executable with `-game <folder>`, your game arguments and `+map`, once the compiled map is in its maps folder (compile it with **Copy to the game**). The reuse of a running game, the timing option, the bridge and the props import are Garry's Mod features.

## MCP

`launch_game` (with `reload: true` to reload in the running game) starts or reloads the map from AI apps. See the [MCP tool reference](../../MCP.md).

## Tips

- Keep the game open between tests: **Reuse the running game** loads the new map in a few seconds.
- Choose **when the compile starts** on a long compile: the game boots during the lighting.
- **Spawn at the editor camera** puts you right where you were looking in the editor, in noclip.

## FAQ

**The game starts a second copy instead of loading the map.** Check that **Reuse the running game** is on and the game was started from the install configured in **Settings > Game**.

**Spawn at the editor camera is greyed out.** Install the bridge first and restart the game.

**Does the bridge go into my released addon?** No, it is never packed into a map or a release.

## Live link

To see entity edits in the running game without recompiling, turn on the [live link with Garry's Mod](live-link.md) (**Tools > Integrations > Start the live link with Garry's Mod**).

## Related pages

- [Compile window](../compiler/compile-window.md)
- [Compile and test in Garry's Mod](../guides/compile-and-test-gmod.md)
- [Game setup](../getting-started/game-setup.md)
- [Build and save as content](build-content-release.md)

---
[Back to the documentation index](../README.md)
