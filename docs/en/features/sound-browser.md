[Documentation](../README.md) > Features > Sound browser | [Français](../../fr/features/sound-browser.md) | [Русский](../../ru/features/sound-browser.md)

# Sound browser

The sound browser lists the sounds of the mounted game content, either the game sounds defined by the soundscripts (`Streetwar.d3_c17_13_horn`) or the sound files (`ambient/alarms/klaxon1.wav`), with a preview player, a volume, autoplay and the soundscript entry of each game sound. The sound keys of the entity panel have their own quick sound picker and can send you to the browser to pick a sound.

## Opening it

- **View > Panels > Sound browser** adds the **Sounds** tab to the right dock (or removes it). Saved as `show_sound_browser` in `[browser]` (default off).
- The browse button of a sound key in the entity panel opens it too (see "Picking a sound for a key" below).
- No default shortcut. With the Hammer++ keymap preset (**Settings > Keyboard**), the shortcut is `Shift+F12`.
- The `🗗` button (**Open in a window**) moves the browser into its own floating window; `⊟` (**Back to the dock**) puts it back in the dock.

## The list

| Control | What it does |
|---|---|
| **Game sounds** / **Files** | What the list shows: game sounds of the soundscripts, or sound files. |
| `⟳` **Reload the soundscripts** | Reads the soundscripts again (after you edited one). |
| **Search sounds (!word excludes)** | Keeps the sounds containing every word (any case). In game sounds mode a word can match the name or the file it plays. A word starting with `!` excludes. |
| Source chips **All**, **Game**, **Addons**, **Workshop** | Keep the sounds of one kind of source (a chip appears only when the content has that kind). |
| **Garry's Mod content only** | Only what every Garry's Mod player has. Shared with the model and particle browsers (`gmod_only` in `[browser]`). |
| **Sounds: N** | Number of sounds listed ("Reading the soundscripts..." while they load). |

Each row has a play button, the name, and in game sounds mode the first wave file it plays, then a badge telling where the file comes from (**GMod**, **game: ...**, **addon**, **Workshop ...**, **custom**, or **not found** when no mounted file matches). The sound playing is in bold. Hover a row for the full source name.

Game sounds come from `scripts/game_sounds_manifest.txt` (its `precache_file`, `preload_file` and `autocache_file` entries), read in the background once per mount and sorted by name. When two soundscripts define the same name, the first one wins.

## Mouse and keyboard

- Click a sound to select it; with **Play on select** on, it plays.
- The play button of a row plays that sound.
- Double click: in pick mode, writes the sound into the key; otherwise plays it.
- While the pointer is over the list: `Up` / `Down` move the selection (and play it with **Play on select**), `Enter` does what a double click does.

## Footer

| Control | What it does | Default |
|---|---|---|
| Selected sound | Its name (or "Select a sound"). | |
| **▶ Play** / **■ Stop** | Plays or stops the selected sound. | |
| Volume slider (0 to 100 %) | **Preview volume** of every sound preview, also the ones of the entity panel. `sound_volume` in `[browser]` (0 to 1). | 80 % |
| **Play on select** | Selecting a sound plays it. `sound_autoplay` in `[browser]`. | On |
| **Soundscript...** | For a game sound: opens a small window with its soundscript entry (the file name first). Hover the button for the soundscript file. | |
| **Copy name** | Copies the selected name to the clipboard. | |
| **Replace all uses (N)** | In pick mode, when the key already has another sound: every entity whose key holds the old sound gets the selected one (one undo step), and pick mode ends. | |

Previews play WAV (PCM and ADPCM), MP3 and OGG files of the mounted content, on Windows only; elsewhere the footer says that no preview is possible. A game sound plays its first wave file.

## Picking a sound for a key

A sound key of the entity panel (for example `message` of `ambient_generic`) has three buttons (see [Entity properties](entity-properties.md)):

- `♪` opens a quick picker: a searchable list of the game sounds (with the file they play) followed by the sound files. Each item has a play button. You can also type a value that is not in the list.
- `▶` / `■` plays or stops the sound the key holds (greyed out when it cannot be played).
- The browse button ("Choose in the sound browser (double click a sound)") opens the sound browser in pick mode.

In pick mode the browser shows a banner "Pick for *key*: double click an item, or select it and press Use." The sound the key already has is selected (the browser switches to **Files** when the value is a `.wav`, `.mp3` or `.ogg` file). Double click a sound, press `Enter` or click **Use** to write it (one undo step); the panel goes back to the **Entity** tab. **Cancel** leaves pick mode.

## Tips

- Turn off **Play on select** to browse a long list quietly with the arrow keys, then press the play button of the row you want.
- The [soundscape editor](soundscapes.md) uses the same quick sound picker.
- Through MCP, `search_sounds` searches the sounds of the content (see the [MCP tool reference](../../MCP.md)).

## FAQ

**Why does the list stay empty?** The game content is not loaded ("Game content is not loaded: set the game folder in the settings"), or the search, the source chips or **Garry's Mod content only** hide everything.

**I edited a soundscript, the browser shows the old entry.** Click `⟳` to read the soundscripts again.

**Why can I not hear anything?** Only WAV, MP3 and OGG files of the mounted content play, on Windows. Check the volume slider, and that the badge does not say **not found**.

## Related pages

- [Entity properties](entity-properties.md)
- [Soundscapes](soundscapes.md)
- [Model browser](model-browser.md)
- [Particles](particles.md)
- [Sound editor](sound-editor.md)
- [Game setup](../getting-started/game-setup.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
