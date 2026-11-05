[Documentation](../README.md) > Features > Themes and languages | [Français](../../fr/features/themes-languages.md) | [Русский](../../ru/features/themes-languages.md)

# Themes and languages

Hammer Pro has a dark theme and a light one, an optional accent color, an interface scale, an optional background image over the window, and its whole interface in English, French and Russian, switchable at any time without a restart.

## Interface icons

Hammer Pro embeds a uniform set of outline icons for its toolbars, tools, dialogs and status indicators. The icons keep their proportions and spacing at every interface scale, independently of the installed Windows symbol fonts. Text continues to use the normal interface font. The bundled icon subset derives from [Microsoft Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons); its MIT license is included in the font and in `assets/fonts/LICENSE.txt`.

## Opening it

Everything on this page is in **Settings > General** (**Tools > Options...**, `Ctrl+,`), sections **Appearance** and **Background image**. The language is also in **View > Language** and the command palette.

## Appearance

The selected light or dark theme is applied from startup, independently of the operating system theme and any previously restored interface state.

| Option | What it does | Default |
| --- | --- | --- |
| **Language** | Language of the interface: English, Français or Русский. Applied at once. | the system language at first start |
| **Theme** | **Dark** or **Light**. Applied at once. Key `ui.theme`. | Dark |
| **Accent color** | **Black and white** (no accent), one of eight swatches (blue, orange, green, purple, red, pink, cyan, yellow), or a **Custom color** from the color picker. It colors highlights, the active tool and the selection of the interface. Key `ui.accent_color`. | Black and white |
| **Interface scale** | Size of the whole interface, 0.75x to 2x by steps of 0.05. Applied when you release the slider. The **1x** button resets it. Key `ui.ui_scale`. | 1x |
| **Show frame rate** | Shows the frame rate. Key `view.show_fps`. | off |
| **Native window frames** | See [Window frames](window-frames.md). | off |

The look is black and white by design: states and severities read from gray levels, weight, icons and inverted fills. Color stays in the content (the views, thumbnails, entity and visgroup colors) and in the accent you choose.

## Background image

An image drawn over the panels of the window (under dialogs and menus), at a low opacity, to make the editor yours.

| Option | What it does | Default |
| --- | --- | --- |
| **Image** | **Choose...** picks a PNG, JPG, BMP, TGA or WebP file; **Remove** takes it away. The file name shows next to the buttons (**No image** when empty). Key `ui.background_image`. | none |
| **Opacity** | 0 to 50%. | 15% |
| **Position** | X and Y offset in pixels from the centered position. | 0, 0 |
| **Zoom** | 0.25x to 4x over the size that just covers the window. **Fit** goes back to covering the whole window, centered. | 1x |
| **Hide it over the 3D view** | The image is not drawn over the 3D views. | on |

The image covers the window and is centered, then moved and zoomed by these settings. Its options only show once an image is chosen.

## Languages

The interface exists in English, French and Russian. Every label, tooltip, message and the compile log notices follow the language. Ways to change it:

- **Settings > General > Language**.
- **View > Language**, then **English** or **Français**.
- The command palette (`Ctrl+P`): type `language` and pick **Language: English** or **Language: Français**.
- The first run wizard asks for it (reopen it with **Help > Setup assistant...**).

The change applies at once, without a restart, and is saved in settings.toml (`language`, `En` or `Fr`). At the very first start the language follows the system: French when the system language is French, English otherwise.

### For one run only

Start Hammer Pro with `--lang fr` or `--lang en` to use a language for this run without changing the saved choice:

```text
hammerpro.exe --lang fr
```

See [Command line](../reference/command-line.md).

## Tips

- On a 4K screen, set **Interface scale** to 1.5x or more; the views keep their full resolution.
- Pick an accent color to spot the active tool and the selection faster.
- Keep the background image opacity low (10 to 20%) so the panels stay readable.

## FAQ

**Can I translate Hammer Pro into another language?** Only English, French and Russian exist in 1.0.2. Every text lives in translation tables in the source; see [Extending Hammer Pro](../development/extending.md).

**The interface is too big after I changed the scale.** Click **1x** next to the slider, or edit `ui_scale` in settings.toml.

**Does the language change the names of files or maps?** No. Only the interface text changes; maps, keys and settings stay the same.

## Related pages

- [Settings window](settings-dialog.md)
- [Window frames](window-frames.md)
- [Command palette](command-palette.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
