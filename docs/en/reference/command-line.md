[Documentation](../README.md) > Reference > Command line | [Français](../../fr/reference/command-line.md)

# Command line

`hammerpro.exe` is the editor, the map compiler and the MCP bridge in one program. This page lists every argument it accepts.

## Usage

```text
hammerpro [map.vmf] [--lang en|fr] [--size WxH] [--screenshot out.png] [--frames N]
          [--maximize-view 0..3] [--tool NAME] [--script file.txt] [--bench-open map.vmf]
hammerpro --mcp
hammerpro --bspc [compiler options] <map.vmf>
hammerpro --rad [lighting options] <map.bsp>
```

`--mcp`, `--bspc`, `--rad` and `--light-preview` must be the **first** argument: they start a mode without any window. Every other argument belongs to the editor and can come in any order.

## Editor arguments

| Argument | What it does | Default |
|---|---|---|
| `map.vmf` | Any argument that does not start with `--` is the map to open at startup (like **File > Open map...**). With several, the last one wins. | none (empty map) |
| `--lang en` / `--lang fr` | Interface language for this session only. The language saved in the settings is not changed. Any other value is ignored. | the saved language |
| `--size WxH` | Size of the window content in points, for example `--size 1920x1080`. The window frame margin is added around it, and the minimum size is 960x600. | `1600x960` |
| `--screenshot out.png` | Automated run: draws N frames (see `--frames`), saves a PNG of the whole window to the given file and closes. | off |
| `--frames N` | Number of frames drawn before the `--screenshot` capture (at least 2). | `90` |
| `--maximize-view N` | Starts with one view maximized. N is the view index: `0` 3D, `1` Top, `2` Front, `3` Side in the default layout. | none |
| `--tool NAME` | Tool active at startup. Names (case ignored): `select`, `camera`, `entity`, `block`, `texture`, `decal`, `overlay`, `clip`, `vertex`, `path`, `polygon`, `curve`, `sprinkle`, `displacement`, `cordon`. An unknown name is ignored. | `select` |
| `--script file.txt` | Automated run: plays the commands of a script file (mouse, keys, actions, screenshots, asserts). See [Script commands](script-commands.md). | off |
| `--bench-open map.vmf` | Automated run: opens the map, waits until it is drawn and the game content has settled, prints the startup and open timings to stdout as `key=value` pairs and closes. | off |
| `--restore-session list.txt` | Opens the maps of a list written by Hammer Pro when it restarts after a graphics device loss (one tab per map, modified maps from their saved changes), then deletes the list. See [Graphics device lost](../troubleshooting/graphics-device-lost.md). | off |

An unknown argument starting with `--` prints `unknown argument: ...` on stderr and is ignored.

## Modes without a window

| Argument | What it does |
|---|---|
| `--mcp` | The MCP bridge that AI apps start themselves. It opens no window and forwards the tool calls to a Hammer Pro window whose MCP server is on. stdout only carries MCP messages. See [MCP server](mcp-server.md). |
| `--bspc [options] <map>` | The Hammer Pro map compiler (geometry, portals, leak test and visibility), with the arguments of VBSP (`-game`, `-leaktest`...). It writes `<map>.bsp` next to the map, `<map>.lin` when the map leaks, `<map>.prt` for a sealed map, and a `<map>.log`. |
| `--rad [options] <map>` | The Hammer Pro lighting, with the arguments of VRAD (`-ldr`, `-hdr`, `-both`, `-final`, `-fast`, `-bounce N`...) plus Hammer Pro's own options (`-nostaticpropshadows`, `-nodispshadows`...). It lights `<map>.bsp` in place. |
| `--light-preview ...` | Internal: the lighting of the baked lighting preview, started by the editor itself. You never need to run it. |
| `--apply-update <zip> <program> <pid>` | Internal: the update helper that the editor starts from a copy of itself when you click **Update**. It has no window: it waits for the process `<pid>` to end, extracts `hammerpro.exe` from `<zip>`, replaces `<program>` (keeping `<program>.old`, rolling back on failure) and starts it. See [Updates](../features/updates.md). |

Every flag of `--bspc` and `--rad`, with its effect and default, is documented in [Compiler command line](../compiler/command-line.md).

## Automated runs

`--script`, `--screenshot` and `--bench-open` make an automated run. It is built to never touch your own files or your game:

- It starts from the **default settings** and never writes `settings.toml`. The first run wizard is not shown.
- Autosave is off, the compiled map is not copied to the game's `maps` folder and the game is never started (compile, launch, game session, publishing: refused with a "skipped: automated runs never write into the game folder or start the game" message).
- No log file, no crash report, no crash recovery prompt, no Discord presence and no update check (unless `HP_UPDATE_API_URL` is set). The thumbnail cache and the per-map sidecar files stay in memory.
- Native window frames are not used (Hammer Pro's own frame is drawn).

Exit codes: `0` normal end, `2` the script file cannot be read or a line cannot be parsed (the error names the line), `3` an `assert_selected` or `assert_tabs` command failed.

## Environment variables

| Variable | What it does |
|---|---|
| `WGPU_BACKEND` | Forces the graphics API, for example `dx12`, `vulkan` or `gl`. Useful when the default one fails on your GPU. |
| `HAMMERPRO_INSTANCE` | For `--mcp`: the process id of the Hammer Pro window the bridge must use. |
| `HAMMERPRO_MCP_DIR` | Moves the MCP instance records (tests and automated checks). |
| `HP_UPDATE_API_URL` | Replaces the GitHub release address of the update check (tests). See [Updates](../features/updates.md). |
| `HP_UPDATE_CURRENT_VERSION` | Pretends another version is installed (tests of the update). |

## Examples

```bat
rem Open a map in French
hammerpro.exe "C:\maps\gm_mymap.vmf" --lang fr

rem Screenshot of the editor with the block tool, 3D view maximized
hammerpro.exe gm_mymap.vmf --tool block --maximize-view 0 --screenshot shot.png --frames 120

rem Compile a map from a batch file
hammerpro.exe --bspc -game "C:\...\GarrysMod\garrysmod" "C:\maps\gm_mymap.vmf"
hammerpro.exe --rad -both -final -game "C:\...\GarrysMod\garrysmod" "C:\maps\gm_mymap.bsp"
```

## Related pages

- [Script commands](script-commands.md)
- [MCP server](mcp-server.md)
- [Compiler command line](../compiler/command-line.md)
- [Files and folders](files-and-folders.md)
- [Themes and languages](../features/themes-languages.md)

---
[Back to the documentation index](../README.md)
