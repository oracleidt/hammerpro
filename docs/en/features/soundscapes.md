[Documentation](../README.md) > Features > Soundscapes | [Français](../../fr/features/soundscapes.md) | [Русский](../../ru/features/soundscapes.md)

# Soundscapes

The soundscape editor writes the soundscape file of your map: the ambient sounds that `env_soundscape` entities play (wind, birds, machines, random distant noises). It edits `soundscapes_<map>.txt` as a list of soundscapes, each with its rules, picks waves from the sound files of the game content and lets you listen to them.

## Opening it

**Map > Soundscapes...** opens the **Soundscapes of the map** window (no default shortcut). It stays open while you work; close it with its close button.

The map must be saved first: the editor works on the file `soundscapes_<map>.txt` next to the map file. For an unsaved map the window says **Save the map first: its soundscape file is written next to it.**

The top of the window shows the file path, **(not saved)** when you have unsaved changes, and a reminder: **The game loads scripts/soundscapes_<map>.txt: pack this file with the map. Use env_soundscape entities to play them.**

## Buttons

| Button | What it does |
|---|---|
| **Save** | Writes the file. Enabled when something changed. The file is written safely (a temporary file, then renamed over the old one) and only when you click **Save**. |
| **Reload** | Reads the file from disk again, dropping unsaved changes. |

If the file cannot be read (for example it is larger than 4 MB), the window shows **The file cannot be read (saving is disabled)** so that saving cannot destroy it.

## The soundscape list

On the left:

- **New soundscape name** and **+** (**Add a soundscape**) add a soundscape with `dsp` set to `1`. A name cannot be empty, contain spaces, quotes or braces, or already exist in the file.
- Click a soundscape to edit it.
- A soundscape whose name is already used by a soundscape of the mounted game content is flagged **name used by the game**. Soundscape names are global in the engine, so rename it.

## Editing a soundscape

On the right, for the selected soundscape:

- **Delete** removes the soundscape.
- **DSP effect** sets its `dsp` key.
- **+ playlooping**, **+ playrandom** and **+ playsoundscape** add a rule.
- Each rule has **×** (**Remove this rule**).

Every field is plain text. Emptying a field removes its key; double quotes are not accepted.

### Rules

| Rule | Fields | Added with |
|---|---|---|
| `playlooping` (a looping sound) | **Volume**, **Pitch**, **Wave**, **Position**, **Sound level** | `volume 1`, `pitch 100`, an empty wave |
| `playrandom` (random sounds at random times) | **Volume**, **Pitch**, **Time (min,max)**, the waves of its `rndwave` block, **Sound level** | `volume 1`, `pitch 100`, `time 10,20`, one empty wave |
| `playsoundscape` (plays another soundscape) | **Volume**, **Soundscape** (name of the other soundscape) | `volume 1`, an empty name |

Each **Wave** row has a text field, a **♪** picker listing the sound files of the game content, a **▶** button that plays the wave, and **×** (**Remove this wave**). **+ wave** adds a wave to a `playrandom` rule. The preview volume is the one of **Settings > Entities and browsers > Sound preview volume**.

Keys left empty (a rule added without its wave yet) are not written to the file, so the engine does not complain about them. Keys and blocks the editor does not know are kept as they are.

## Tips

- Name your soundscapes with a map prefix (for example `gm_mymap.forest`) to avoid the **name used by the game** warning.
- The engine reads `scripts/soundscapes_<map>.txt`: place or pack the file there for the game to use it. See [Build and save as content](build-content-release.md) and [Map assets](map-assets.md).
- Pick a wave with the **♪** button rather than typing it: the list only holds files that exist in the mounted content.
- Through MCP, `run_action` with `edit_soundscapes` opens the window. See the [MCP tool reference](../../MCP.md).

## FAQ

**Why is the window empty with a message?** The map has never been saved, so there is no folder to write the soundscape file to. Save the map, then open the editor again.

**Why is Save greyed out?** Nothing changed since the last save or reload, or the file could not be read (saving is then disabled to protect it).

**My soundscape does not play in game.** Check that an `env_soundscape` entity uses its name, that the file is in `scripts/` as `soundscapes_<map>.txt` (packed with the map), and that the name is not flagged as used by the game.

**Can I edit the soundscapes of the game itself?** No. The editor only edits the file of your map. The game's soundscapes are read to warn about name collisions.

## Related pages

- [Sound browser](sound-browser.md)
- [Entity properties](entity-properties.md)
- [Map assets](map-assets.md)
- [Build and save as content](build-content-release.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)

## Assigning a custom soundscape

For `env_soundscape` and `env_soundscape_triggerable`, type the script entry name directly in the **Soundscape** field, for example `sunless_cavesound_entree`, then press Enter. The adjacent button offers FGD choices and names from mounted soundscape scripts. Custom names are preserved when saving the VMF and support Undo/Redo.
