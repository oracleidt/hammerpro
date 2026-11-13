[Documentation](../README.md) > Features > Soundscapes | [Français](../../fr/features/soundscapes.md) | [Русский](../../ru/features/soundscapes.md)

# Soundscapes

Soundscapes are the ambient sounds of a Source map: wind, birds, machines, random distant noises. An `env_soundscape` entity names a soundscape of the game or of the map file `scripts/soundscapes_<map>.txt`, and the engine plays the soundscape of the nearest entity the player is in range of and can see.

Hammer Pro has three tools for them. None of them runs until you turn it on:

| Tool | Where | What it does |
|---|---|---|
| **Soundscape editor** | **Map > Soundscapes...** (action `edit_soundscapes`) | Lists the soundscapes of the game and of the map, plays them, edits and saves the soundscape file of the map, assigns a soundscape to the selected `env_soundscape`. |
| **Soundscape view** | **View > Environment preview > Show soundscapes** (action `toggle_soundscape_view`, off by default) | Draws the soundscape entities, their radius, their position targets and names, the `ambient_generic` radii, and colors the floor with the soundscape the player would hear there. |
| **Hearing in the player view** | **View > Environment preview > Hear soundscapes in the player view** (action `toggle_soundscape_hearing`, off by default), or `K` in the player view | Plays the active soundscape and the `ambient_generic` sounds while you walk in the player view, with distance and soundscape changes like the engine. |

**View > Environment preview > Soundscape options** holds the volume slider, **Mute**, **Active areas** and **ambient_generic sounds**.

## The soundscape editor

**Map > Soundscapes...** opens the **Soundscape editor** window; the same entry closes it.

### The file

The top of the window shows the file the map soundscapes are saved to: `scripts/soundscapes_<map>.txt` under the folder above the `maps` folder of the map (your addon or project folder), or under the game folder when the map is not in a `maps` folder. That is the file the engine loads for the map, so it works in game as it is, and it is packed with the map by [Build and save as content](build-content-release.md).

- **Folder...** picks another game or addon folder; **Default folder** goes back.
- **(not saved)** shows when you have changes. They are kept while Hammer Pro runs (also when you close the window or switch maps), and the preview, the hearing and the MCP tools already use them, but the file only changes when you click **Save**.
- **Save** writes the file safely (a temporary file renamed over the old one). A file listed by the game's `scripts/soundscapes_manifest.txt` is never written: the editor only writes the file of the map.
- **Reload** reads the file again and drops the unsaved changes.
- The map must be saved first (the file is named after it).
- Older versions of Hammer Pro wrote `soundscapes_<map>.txt` next to the map file, where the game does not read it. Such a file is read when the `scripts` one does not exist yet, the window tells you, and **Save** writes it into `scripts/` (the old file is left alone).

### The list

On the left, the soundscapes of the map file, then **Game soundscapes**: those of every file of `scripts/soundscapes_manifest.txt` in the mounted content. **All**, **Map** and **Game** filter the list and the search box keeps the names containing every word.

- **New soundscape name** and **+** add a soundscape to the map file (with `dsp` 1). Names cannot hold spaces, quotes or braces.
- A map soundscape with the name of a game soundscape is shown in the warning color: soundscape names are global in the engine, so rename it.
- Game soundscapes are read only. **Copy into the map file** copies one under a new name (`<map>.<name>`) so you can change it.

### A soundscape

On the right:

| Control | What it does |
|---|---|
| **Preview** / **Stop** | Plays the soundscape with its nested soundscapes and its random sounds at random times, without distance, until **Stop**. |
| **Use on the selection (n)** | Sets the `soundscape` key of the selected `env_soundscape` and `env_soundscape_triggerable` entities (one undo step **Assign soundscape**). |
| **Used by** | The soundscape entities of the map naming it; click one to select it. |
| Summary line | How many looping and random sounds it plays with its nested soundscapes, its DSP, nested names that do not exist, nesting loops. |
| **DSP effect** | The `dsp` room effect the engine switches to when this soundscape starts (0 none, 1 generic room, 2 to 28 the standard Source rooms; the name of the preset is shown). Only the soundscape an entity names sets it, nested ones do not, and without the key the previous room effect stays. **×** removes the key. |
| **Player DSP** | `dsp_player`: a second effect the engine applies on top of the room effect (same numbers), kept until another soundscape sets it. |
| **Effect volume** | `dsp_volume`: how loud the room effect is (1 by default). |
| **+ playlooping**, **+ playrandom**, **+ playsoundscape** | Add a rule. Each rule has **×** to remove it. |

Rules:

| Rule | Fields |
|---|---|
| `playlooping`: a sound that loops | **Volume**, **Pitch**, **Wave**, **Sound level**, **Position** |
| `playrandom`: random sounds at random times | **Time (seconds)**, **Volume**, **Pitch**, its waves, **Sound level**, **Position** (also **random around the player**) |
| `playsoundscape`: plays another soundscape | **Soundscape** (type it or pick it, **Open** shows it), **Volume** (multiplies the volumes of the nested soundscape), **Position offset** (`position`, added to its position numbers), **Position override**, **Ambient position override** |

- **Volume**, **Pitch** and **Time** take one number or, with **range** ticked, a range: the engine picks a random value in it each time.
- **(default)** after a field means the key is not in the file and the engine default applies, like in game: a `playlooping` without a volume plays nothing, a `playrandom` without volume plays nothing, without pitch is ignored by the engine and without time plays again every frame. The editor warns about each of them; new rules are added with working values.
- **Wave**: type the path under `sound/`, or **♪** picks a sound file of the mounted content (search box in the list), **▶** plays it, **×** removes it. A wave missing from the content is flagged, and so is a `playlooping` wave without loop markers (the game plays it once; the preview repeats it).
- **Sound level** sets how far a positioned sound carries (`SNDLVL_NORM` is 75 dB). Ambient sounds ignore it.
- **Position**: **none (ambient)** plays the sound everywhere at the same level. **position0** to **position7** play it from the entity named by the `position0` to `position7` key of the `env_soundscape`, fading with distance. The view draws those targets.

Keys and blocks the editor does not know are kept as they are. Empty keys (a rule added without its wave yet) are not written.

### Hearing options

The second row of the window holds **Show in the views**, **Active areas**, **Hear in the player view**, **ambient_generic sounds**, **Volume** and **Mute** (the same settings as the menus). While the player view hears soundscapes, the window shows the one playing and why.

## The soundscape view

With **Show soundscapes** on, every view draws:

- each `env_soundscape`, `env_soundscape_proxy` and `env_soundscape_triggerable` with a circle of its `radius` (three circles in 3D) in a color of its soundscape name, a label with the soundscape name and radius (**everywhere** for radius -1), **(disabled)** for Start Disabled, the main entity of a proxy, and **(by trigger_soundscape)** for the triggerable ones;
- lines from the entity to its `position0` to `position7` targets, labelled **position0** to **position7**;
- each `ambient_generic` with a light blue circle of its `radius`, labelled with its sound file (**(starts silent)** when it does);
- each `trigger_soundscape` as a box in the color of the soundscape of the `env_soundscape_triggerable` it names, a line to that entity and a label (or a warning when no `env_soundscape_triggerable` has that name);
- with **Active areas** on (default), floor tiles in the color of the soundscape the player would hear standing there, at the height of the 3D camera (or of the player in the player view), by the engine rules below. Inside a `trigger_soundscape` with no other soundscape in range and in sight, the tile takes the soundscape of its triggerable entity. Grey tiles are out of every radius: the player keeps hearing the soundscape heard before. Tiles fill in over a few frames after a change;
- in the 3D views, a corner label **Soundscape here** with the soundscape heard at the camera.

How the engine picks the soundscape (the Source SDK 2013 code of `env_soundscape` and the soundscape system, matched exactly):

- Every server frame the soundscape heard now is tested first, then the other soundscape entities (`env_soundscape`, `env_soundscape_proxy` and `env_soundscape_triggerable`). A disabled current soundscape is dropped; disabled ones are skipped.
- An entity is in range when its `radius` is greater than its distance to the player's ear (strictly), or when the radius is exactly `-1`. Any other negative radius, a radius of 0 and a missing radius (0) never reach anyone.
- In range, it must also see the ear: a trace from the entity's origin to the player's ear (the eyes: 64 units above the feet standing, 28 crouched) with the mask `MASK_SOLID_BRUSHONLY | MASK_WATER`. Solid world brushes, brush entities, windows, grates, water and solid `prop_static` collision block it; clips, triggers, `func_illusionary`, other props and players do not. An entity whose origin is inside a solid brush, or exactly on its surface, starts in solid and never sees anyone.
- A candidate that passes takes over when the current soundscape did not pass, or when it is nearer. When nothing passes, the current soundscape stays.
- An `env_soundscape_proxy` plays the soundscape and the position targets of the entity named by its `MainSoundscapeName`.
- A `trigger_soundscape` the player enters makes the `env_soundscape_triggerable` named by its `soundscape` key current at once (the last entered wins); leaving it goes back to another trigger the player is still in, or to no soundscape. The radius rules above still run every frame, so a soundscape in range and in sight takes over again. When the server has no soundscape the client keeps playing the last one.
- The engine only tests the soundscapes whose visibility (PVS) reaches the player's cluster; a clear trace inside the radius always satisfies that, so the result is the same.

## Hearing in the player view

Turn on **Hear soundscapes in the player view**, enter the player view (F6) and walk. Hammer Pro runs the client soundscape system of the engine:

- When the soundscape changes, every looping sound moves toward its new volume by a third of full volume per second (`soundscape_fadetime`, 3 seconds); a looping sound with the same wave and pitch in the new soundscape keeps playing at its new volume. Ambient loops start silent, positioned loops at 0.05.
- `playrandom` sounds start after half a random interval, then wait a random `time` between plays, with their random volume, pitch and sound level; `position random` plays them 36 units around the eyes, like the engine.
- Positioned sounds fade with distance by their sound level and are panned left or right; ambient sounds play at their volume everywhere.
- The room effect (`dsp`), `dsp_player` and `dsp_volume` of the soundscape are applied: a reverb, echo and filter close to each standard Source preset (rooms, metal, tunnels, chambers, bright rooms, under water, concrete, outside, caverns, weird), crossfaded over 3 seconds. Values above 28 play as the generic room.
- `trigger_soundscape` volumes activate their `env_soundscape_triggerable` as you walk through them.
- With **ambient_generic sounds** on, the `ambient_generic` entities play like at map start: looping ones continuously unless Start Silent, the others once; `radius` becomes a sound level like the engine, `health` the volume, `pitch`, Play Everywhere, Source Entity Name. Their `PlaySound`, `StopSound`, `ToggleSound`, `Volume`, `Pitch`, `FadeIn` and `FadeOut` inputs, and the `Enable`, `Disable` and `ToggleEnabled` inputs of the soundscape entities, sent by the player view's I/O simulation, are followed.

`K` turns hearing on or off inside the player view; the counters show the soundscape playing, the sounds and the volume. `Soundmixer` keys only change the balance with game sounds (weapons, voices) the editor does not play. Playback uses the default audio device on Windows; the audio output is released ten seconds after the last sound.

## MCP

Everything is reachable through the tools (see the [MCP tool reference](../../MCP.md)):

| Tool | Use |
|---|---|
| `list_soundscapes` | The game and map soundscapes and every soundscape entity of the map. |
| `get_soundscape` | A soundscape with typed rules, its KeyValues text, the flattened sound list and the waves found. |
| `edit_soundscape` | Create, change (`rules`, `add_rules`, `dsp`, `dsp_player`, `dsp_volume`, `rename`), copy (`copy_from`) or delete a map soundscape, assign it (`assign_to`), `dry_run`. |
| `preview_soundscape` | Plays a soundscape or stops; `record_to` records what the soundscape output sends to the audio device into a WAV file, `record_status` tells when it is written with its peak and RMS levels. |
| `soundscape_at` | The soundscape heard at a point, every candidate with distance, range and line of sight, and the audible `ambient_generic` sounds. |

Actions (`run_action`): `edit_soundscapes`, `toggle_soundscape_view`, `toggle_soundscape_hearing`. Settings (`set_setting`): `lab.soundscape.view`, `lab.soundscape.areas`, `lab.soundscape.hearing`, `lab.soundscape.volume`, `lab.soundscape.mute`, `lab.soundscape.ambient_generic`.

## Tips

- Name map soundscapes with a map prefix (`gm_mymap.forest`) so no game soundscape has the same name.
- Give `playlooping` rules looping waves (the sound editor can add loop markers: [Sound editor](sound-editor.md)).
- Turn on the view and look at the floor tiles: a room heard as the outdoor soundscape usually needs an `env_soundscape` inside it, nearer than the outdoor one.

## FAQ

**My soundscape does not play in game.** Check that an `env_soundscape` uses its name (the view labels it), that the player is within its radius and sees it (the floor tiles tell), and that the file is `scripts/soundscapes_<map>.txt` in the game or addon folder, or packed with the map.

**Why does the old soundscape keep playing in a corridor?** Out of every radius the engine keeps the current soundscape. Add an `env_soundscape` covering the corridor.

**Can I edit the soundscapes of the game?** No, they are read only. Copy one into the map file and change the copy.

**An env_soundscape never plays although the player is close.** Its origin is probably inside a brush or exactly on the floor: the engine trace then starts in solid and fails. Raise it a few units above the floor; `soundscape_at` reports `start_solid` for each candidate.

**I hear nothing in the player view.** Hearing is off by default: turn it on (or press `K` in the player view), check **Mute** and the volume, and that the waves exist (the editor flags missing ones).

## Related pages

- [Player view](player-view.md)
- [Sound browser](sound-browser.md)
- [Sound editor](sound-editor.md)
- [Entity properties](entity-properties.md)
- [Map assets](map-assets.md)
- [Build and save as content](build-content-release.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)

## Assigning a custom soundscape

For `env_soundscape` and `env_soundscape_triggerable`, type the script entry name directly in the **Soundscape** field, for example `sunless_cavesound_entree`, then press Enter. The adjacent button offers FGD choices and names from mounted soundscape scripts. Custom names are preserved when saving the VMF and support Undo/Redo. The soundscape editor's **Use on the selection** does the same for every selected soundscape entity.
