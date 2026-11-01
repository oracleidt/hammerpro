[Documentation](../README.md) > Features > Build and save as content | [Français](../../fr/features/build-content-release.md)

# Build and save as content

**Build and save as content** turns a Garry's Mod map into a finished release in one window: a Final compile, the cubemaps and the navigation mesh built in game if you want, every needed file found and packed, a Workshop addon (`addon.json`, content folder and `.gma`), a thumbnail and a Workshop icon, a checklist, and publishing on the Workshop with the game's own `gmpublish`.

It is a Garry's Mod feature. For a map of another game the window says so: compile it from the compile window (`F9`), the compiled map goes to the maps folder of its game.

## Opening it

| Command | Opens |
| --- | --- |
| **File > Build and save as content...** | The release window on the **Build** tab. |
| **File > Publish on the Workshop...** | The release window on the **Publish** tab. |

The release is built from the saved file: when the map has unsaved changes, the window asks you to save first (**Save the map...**). The window has five tabs: **Build**, **Checklist**, **Addon**, **Publish** and **Report**.

## Build tab

| Control | What it does | Default |
| --- | --- | --- |
| **Map** | The map being released. | |
| **Output folder** | Where the release is written. **Default** is `<map folder>/<map>_content`; **Change...** picks another folder; **Open folder** opens it. Remembered per map. | `<map>_content` |
| **Produce** | The release mode (below). | **Workshop addon** |
| **Final compile first (geometry, visibility, LDR and HDR lighting)** | Compiles the map with the Final preset before packing. | on |
| **Compress the pakfile (LZMA)** | Smaller BSP file. Garry's Mod reads it but prints a console warning for every compressed file it loads, and Workshop and FastDL downloads are compressed anyway: leave it off unless you need a smaller file. Sounds always stay uncompressed. | off |
| **Keep the built cubemaps of the previous BSP** | Copies the cubemaps already built into the new BSP. | on |
| **Write the .gma** | Writes the addon file. | on |
| **Ship content from Workshop items** | Off: the Workshop items are listed as Required Items instead (players subscribe to them). | on |
| **Ship content from mounted games** | Content of games mounted through `mount.cfg` (CS:S, HL2 episodes...). Players without them see errors; redistributing it is your responsibility. | off |
| **Build cubemaps and navigation in game** | Starts Garry's Mod, after a confirmation, to build the cubemaps and generate the navigation mesh, then closes it. | off |
| **Open the output folder when done** | | off |
| **Protect your BSP** | Ships a protected copy and keeps the full map for your server. See [Protect your BSP](bsp-protection.md). | off |
| **Build** | Runs the release. **Cancel** stops it. | |
| **Analyze** | Lists every file the map needs, without compiling or writing anything (fills the Report tab). | |

These options are stored in the `[content]` table of settings.toml (see the [Settings reference](../reference/settings.md)).

### Release modes

| Mode | What it produces |
| --- | --- |
| **Workshop addon** | The BSP, a content folder and a `.gma`. Files gmad refuses (soundscapes, particle manifest) travel inside the BSP pakfile. |
| **Self-contained BSP** | One BSP with every needed file inside its pakfile. |
| **Server kit** | A self-contained BSP plus a `fastdl` folder (the BSP and its `.nav`) and `lua/autorun/server/<map>_resources.lua`, which adds the Workshop item (`resource.AddWorkshop`, once published) or the map (`resource.AddFile`) to the downloads. |

### What a build does

The stages show while it runs: **Dependencies**, **Compile**, **Game session**, **Pakfile**, **Navigation**, **Content folder**, **GMA**, **Done**. The log below lists what happened (files packed, cubemaps kept, files left out of the GMA, warnings about mounted games and soundscripts).

Output, where `<addon>` is the addon title in lowercase (the map name when the title is empty):

```text
<output>/<addon>/addon.json
<output>/<addon>/maps/<map>.bsp, <map>.nav, thumb/<map>.png
<output>/<addon>/materials/..., models/..., sound/...   (Workshop addon)
<output>/<addon>.gma
<output>/<addon>_icon.jpg
<output>/fastdl/...                                     (server kit)
<output>/.hammerpro_release.toml                        (incremental state)
```

Rebuilds are incremental: `.hammerpro_release.toml` records every file a release wrote, unchanged files are skipped, and the GMA is rewritten only when its inputs changed. Only files a previous release wrote are ever deleted.

The release also adds the Lua of the skybox switches (`point_hammerpro_skybox`) to the addon when the map uses them, since the game never runs Lua from a map's pakfile.

A build stops with a clear message when the map is not saved, the game or its content is not found, the compile fails, the map leaks, needed files are missing (see the report) or some files cannot be loaded from a map or an addon.

### Game session

With **Build cubemaps and navigation in game**, the build asks **Start Garry's Mod?** (close the game first if it is running and do not use it until the build is done; the map is copied into the maps folder of the game). Then:

- **Cubemaps**: the game runs `-buildcubemaps` once per lighting range of the BSP (LDR and HDR) and quits by itself.
- **Navigation**: the game loads a copy of the map that starts `nav_generate`, Hammer Pro waits for the mesh to be saved, then closes the game.

Your video settings and `cfg/config.cfg`, which the game rewrites when it quits, are saved before the session and put back after it, whatever happens. See [Cubemaps](../compiler/cubemaps.md).

## Checklist tab

Every row should be green before publishing (**Refresh** reads everything again). Rows are green, red, orange or unknown, with a fix button when there is one:

| Row | Checks | Fix button |
| --- | --- | --- |
| **Map name** | Lowercase letters, digits, `_` and `-` only; no other addon has a map with this name. | |
| **Full compile** | Saved, compiled, up to date with the map, with visibility. | **Build** |
| **Final lighting** | LDR and HDR lighting, not fullbright. | **Build** |
| **No leak** | The map is sealed. | **Build** |
| **Cubemaps** | Built cubemaps against `env_cubemap` count. | **Build in game** |
| **Navigation mesh** | Present and current, missing, or built for other geometry. | **Build in game** |
| **Dependencies** | Every file found, or the number missing. | **Show** (Report tab) |
| **Map thumbnail** | `maps/thumb/<map>.png` exists. | **Capture** |
| **Workshop icon** | The icon exists (written by the next build from the thumbnail). | **Capture** |
| **addon.json** | Title, type and tags valid. | **Edit** (Addon tab) |
| **Engine limits** | Highest limit and its percentage. | |
| **Gamemode** | What the map prefix promises (below). | |

Gamemode profiles, chosen from the map name prefix:

| Prefix | Gamemode | Needs |
| --- | --- | --- |
| `gm_` | Sandbox | a player spawn (`info_player_start`, `info_player_deathmatch`, `info_player_combine` or `info_player_rebel`) |
| `ttt_` | Trouble in Terrorist Town | `info_player_*`, and weapons or ammo (`weapon_*`, `item_*`, `ttt_random_weapon`, `ttt_random_ammo`) |
| `ph_` | Prop Hunt | `info_player_terrorist` and `info_player_counterterrorist` |
| `rp_` | DarkRP | a player spawn |
| `zs_` | Zombie Survival | `info_player_*` and a navigation mesh |
| `mu_` | Murder | `info_player_*` |
| `deathrun_` | Deathrun | `info_player_terrorist` and `info_player_counterterrorist` |
| `jb_` | Jailbreak | `info_player_terrorist` and `info_player_counterterrorist` |
| `bhop_` | Bunny Hop | `info_player_*` |
| `surf_` | Surf | `info_player_*` |

## Addon tab

| Field | Notes |
| --- | --- |
| **Title** | Plain ASCII, 128 characters at most. Empty: the map name. |
| **Type** | Always `map`. |
| **Tags (2 at most)** | `fun`, `roleplay`, `scenic`, `movie`, `realism`, `cartoon`, `water`, `comic`, `build`. |
| **Ignore** | One gmad wildcard per line, for example `*.psd`. |
| **Description** | Text of the addon. |

**Thumbnail and Workshop icon**: written as `maps/thumb/<map>.png` (256 x 256) and as the 512 x 512 JPEG icon of the Workshop. **Capture the 3D view** uses the center square of the 3D view (frame the map first; the frame shows the safe area), **Choose an image...** loads a file, **Clear** removes it.

The title, description, tags, output folder and Workshop id are kept per map in its sidecar file.

## Publish tab

Shows the **Workshop item** (its id, or *Not published yet*), the **Addon file** and the **Icon**, and a **Change note** field.

- **Publish as a new item...** creates a new public Workshop item with your Steam account.
- **Update the item...** uploads a new version of the item already published.

A confirmation (**Publish on the Workshop?**) comes first; Steam must be running. Hammer Pro runs only the `gmpublish` of your Garry's Mod install (`bin/win64`, then `bin`), never a program found elsewhere. Its output is streamed into the log and **Cancel** closes it. Build the release first (*Build the release first.*).

## Report tab

Every file the map needs with its size, origin, destination (**Pakfile**, **Addon**, **Not shipped**, **Not loadable**) and why it is needed (brush faces, an entity key, an output, an instance). Filter it with **Filter files...**, and tick **Show files that are not shipped** to see the base game files too. Sections:

- **Origins**: files per origin (base game, addons, Workshop, custom folders, downloads, mounted games, extra content, loose game files, generated), with the ship choices for Workshop and mounted content.
- **Workshop items (N)**: the items the map uses, with **Copy the ids** (list them as Required Items when their content is not shipped).
- **Missing (N)**: files the map names but no mounted content has.
- **Paths (N)**: shipped files in generic folders, with spaces in the path or also provided by another addon.
- **Size advice (N)**: textures that could be smaller (DXT5 without alpha, larger than 1024, no mipmaps, uncompressed).

## MCP

`get_dependency_report`, `open_release_window`, `start_release`, `get_release_checklist`, `get_release_status`, `cancel_release`, `inspect_archive` and `publish_workshop` (which opens the Publish tab: the upload always needs your confirmation). See the [MCP tool reference](../../MCP.md).

## Tips

- Click **Analyze** early: the report shows missing files and Workshop dependencies without a compile.
- Use **Self-contained BSP** for servers that do not use the Workshop, **Server kit** when you run the server yourself.
- The checklist fixes jump to the right place: **Build in game** ticks the game session option for you.

## FAQ

**Why are soundscapes inside the BSP even in Workshop addon mode?** gmad refuses those files in a GMA, so they travel in the pakfile, where the game loads them.

**Where is the Workshop id kept?** In the map's sidecar file, so the next publish updates the same item.

**The build says files cannot be loaded from a map or an addon.** Some files (soundscripts outside `maps/<map>_level_sounds.txt`, soundscapes outside `scripts/soundscapes_<map>.txt`) are never loaded for a map. Move their entries as the log explains.

## Related pages

- [Protect your BSP](bsp-protection.md)
- [Map assets](map-assets.md)
- [Collect content for a release](../guides/collect-content-for-release.md)
- [Cubemaps](../compiler/cubemaps.md)
- [Game launch](game-launch.md)

---
[Back to the documentation index](../README.md)
