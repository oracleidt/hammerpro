[Documentation](../README.md) > Features > Command palette | [Français](../../fr/features/command-palette.md) | [Русский](../../ru/features/command-palette.md)

# Command palette

The command palette runs any command of Hammer Pro by typing a few letters of its name: every action of the menus, every tool, the commands that have no menu, your recent maps and the interface languages.

## Opening it

**Tools > Command palette...** or `Ctrl+P`. The palette opens over the window with the search field focused (*Type a command, tool or map...*).

## Using it

| Key | What it does |
| --- | --- |
| Type | Filters the list with a fuzzy search. |
| `Up` / `Down` | Moves the highlight. |
| `Page Up` / `Page Down` | Moves the highlight by a page. |
| `Enter` | Runs the highlighted entry. |
| `Esc` | Closes the palette. |

A click on an entry runs it too. The footer reminds you of the keys (*navigate, run, close*). When nothing matches, the palette says **Nothing found**. At most 200 results are listed.

Each row shows an icon, the name, where it lives (its menu, **Tool** for the tools, **Recent maps** for a map, **View** for a language) and its shortcut when it has one. Commands that cannot run right now (nothing selected, for example) are greyed out and do nothing.

## What you can find

| Entry | Found by |
| --- | --- |
| Every action of Hammer Pro | Its name in the interface language, its English name, or its id with spaces (`toggle grid`). This includes the commands that are in no menu, such as **Gizmo in local space** or **Make hint brush**. |
| Every tool | Its name (for example **Clipping tool**). |
| **Open: map name** | One entry per recent map: its file name, its full path, or `open`. |
| **Language: English** / **Language: Français** | `language`, the language name, or `lang en` / `lang fr`. |

With an empty search, the palette lists the six commands you ran most recently first (during this session), then everything else.

## Tips

- The palette is the fastest way to reach a command without a shortcut: type 3 or 4 letters and press `Enter`.
- English names always work, even when the interface is in French, so a tutorial written in English still helps.
- To give a command a shortcut, see [Keyboard shortcuts](keyboard-shortcuts.md).

## FAQ

**A command is greyed out in the palette.** It cannot run in the current state, like its menu entry. Select something or open a map first.

**Are the recent commands kept after a restart?** No, the list of recent commands lasts for the session.

**Can I search by shortcut?** Not in the palette; the shortcut reference (`F1`) searches by shortcut.

## Related pages

- [Keyboard shortcuts](keyboard-shortcuts.md)
- [Actions and shortcuts](../reference/actions-shortcuts.md)
- [Interface tour](../getting-started/interface-tour.md)

---
[Back to the documentation index](../README.md)
