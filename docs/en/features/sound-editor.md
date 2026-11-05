[Documentation](../README.md) > Features > Sound editor | [Français](../../fr/features/sound-editor.md) | [Русский](../../ru/features/sound-editor.md)

# Sound editor

The sound editor opens a sound of the game content, the addons, the map or your disk, shows its waveform, lets you change its volume, cut it, fade it and make it loop, plays the result, and saves it as a WAV file in the folder you choose. The saved file is always a 16 bit PCM WAV, the format the Source engine reads: MP3 and OGG files can be opened, and are written back as WAV.

## Opening it

- **Tools > Sound editor...** (action `sound_editor`) opens the window, and again closes it.
- MCP: `open_media_editor` with `editor` set to `sound` (and a `path` to load a sound).

## Picking a sound

The list on the left has two sources:

| Source | What it lists |
|---|---|
| **Game sounds** | The sound files (WAV, MP3, OGG) of the mounted game content, the addons and the Workshop archives, the same index as the [sound browser](sound-browser.md) and the MCP tool `search_sounds`. |
| **Used by the map** | The sound files named by the key values of the entities of the open map (`ambient_generic` messages, trigger sounds, any key value ending in `.wav`, `.mp3` or `.ogg`). |

The search box keeps the sounds containing every word. **Open a sound file...** picks any WAV, MP3 or OGG file on your disk.

## The editor

The line under the name gives the sample rate, the channels, the duration, the peak level and the size. Source plays sounds at 11025, 22050 and 44100 Hz correctly; for any other rate a warning suggests the **Sample rate** control.

| Control | What it does |
|---|---|
| Waveform | The kept part is drawn in the accent color, the cut parts are dimmed. Drag near the green line to move the start, near the red line to move the end. The yellow line is the playback position, the blue line the loop start. |
| **Play** / **Stop** | Plays the edited sound (looping when **Loop the sound** is on). **Original** plays the unedited sound. **Volume** is the volume of the preview only. |
| **Reset** | Goes back to the unedited sound (keeping the loop markers it came with). |
| **Volume (gain)** | Gain from -40 to +24 dB. The small figure is the peak level after the gain; the output is clipped at 0 dBFS. |
| **Normalize to** + **Set the gain** | Sets the gain so the loudest sample of the kept part reaches the level (default -1 dB). |
| **Start**, **End**, **Length** | The kept part, in seconds. Changing the length moves the end. |
| **Fade in**, **Fade out** | Linear fades at the ends of the kept part. |
| **Loop** | **Loop the sound** writes the loop markers the engine reads to repeat the sound: a `cue ` chunk and a `smpl` chunk with one forward loop from the loop start to the last sample. **from** is the loop start in seconds from the start of the kept part (default 0). Sounds that already carry loop markers open with the loop on. |
| **Channels** | **Mix down to mono** for a stereo sound (a mono sound plays in 3D positions with `snd_*` effects, a stereo one does not). |
| **Sample rate** | **Keep**, 11025, 22050 or 44100 Hz (linear resampling). |

## Saving

- **Folder** is the root the file is written under. The default is the folder above the `maps` folder of the saved map (your addon or project), otherwise the game folder. **Browse...** picks another folder.
- **File** is the path under that folder, `sound/` is added when missing and the extension is always `.wav`. The line below shows the path that will be written. The default is the path of the source sound with the `.wav` extension, so an edited copy of `sound/ambient/wind.mp3` in your addon folder replaces the original in game.
- If the file exists you must tick **Replace the existing file**.
- **Save as WAV** writes the file and refreshes the content index so the new sound shows in the browsers.

Files written to the game folder are not part of an undo step.

## MCP

The editor is fully reachable through the tools (see the [MCP tool reference](../../MCP.md)):

| Tool | Use |
|---|---|
| `sound_info` | Rate, channels, duration, peak, loop markers, waveform. |
| `sound_edit` | The whole editor in one call: `gain_db` or `normalize_db`, `start` with `end` or `length`, `fade_in`, `fade_out`, `looped`, `loop_start`, `mono`, `sample_rate`, `output`, `root`, `overwrite`, `dry_run`. |
| `sound_preview` | Plays the edit, or stops (`stop`). |
| `list_map_sounds` | The "Used by the map" list. |
| `search_sounds` | The game sounds list. |
| `open_media_editor` | Opens the window, optionally with a sound loaded. |

## See also

- [Sound browser](sound-browser.md)
- [Soundscapes](soundscapes.md)
- [Color correction](color-correction.md)
