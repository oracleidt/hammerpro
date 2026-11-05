[Documentation](../README.md) > Features > Undo and history | [Français](../../fr/features/undo-history.md) | [Русский](../../ru/features/undo-history.md)

# Undo and history

Every edit in Hammer Pro is an undo step with a name: tool edits, menu commands, settings that change the map, and the edits made by AI assistants through MCP. You can undo and redo them one by one, jump to any point of the history, and, by default, undo selection changes too.

## Opening it

| Action | Menu | Default shortcut |
| --- | --- | --- |
| **Undo** | **Edit > Undo** | `Ctrl+Z` |
| **Redo** | **Edit > Redo** | `Ctrl+Y` |
| **Undo history** | **View > Panels > Undo history** | none |

The toolbar has **Undo** and **Redo** buttons. The Edit menu names the step: **Undo Carve**, **Redo Move**... (in French "Annuler : ...").

## Undo steps

- Each step is named after what it did ("Carve", "Paste", "Insert prefab", "Edit cordons", "MCP: move objects"...), in the language of the interface.
- Consecutive changes of the same kind made within 1.5 seconds merge into one step (for example the moves of a color picker).
- Cordons and the camera positions saved in the map are undone with the edits.
- The history is per map: each open map has its own.

## Selection undo

With **Undo restores the selection** on (default), selection changes are undo steps too: `Ctrl+Z` brings back the previous selection (faces of the texture tool included) and `Ctrl+Y` selects again. These steps are named **Selection** or **Face selection**. They never mark the map as modified, consecutive changes in one gesture (a click, a box select, a key) merge into one step, and they are the first steps dropped when the history is full. Selecting what an edit just created joins the edit's step.

Turn it off to undo only edits, like Hammer.

## The Undo history tab

**View > Panels > Undo history** adds the **History** tab to the right dock (hidden by default, `workflow.show_history`):

- the first row, **Map opened**, is the map as it was loaded;
- then the steps done, oldest first, the current one highlighted;
- then the undone steps, in italics.

Click a row to go back or forward to it ("Click a step to go back or forward to it."): Hammer Pro undoes or redoes every step in between.

## Limits

| Option | Where | What it does | Default |
| --- | --- | --- | --- |
| **Undo levels** (`edit.undo_levels`) | **Tools > Options... > Editing** | Most undo steps kept, 10 to 10000. | 500 |
| **Undo restores the selection** (`edit.undo_restores_selection`) | **Tools > Options... > Editing** | Selection changes are undo steps. | On |

The history also keeps its memory under about 512 MB: the oldest steps are dropped first, the most recent step is always kept.

## Tips

- Use the History tab to compare before and after: click **Map opened**, look, then click the last step to come back.
- MCP tools: `undo`, `redo`, `undo_steps`, `redo_steps`, `get_history` (see the [MCP tool reference](../../MCP.md)). Every MCP edit is one step named "MCP: <tool>" you can undo yourself.

## FAQ

**Ctrl+Z only changed the selection.** **Undo restores the selection** is on: the last step was a selection change. Press `Ctrl+Z` again, or turn the option off.

**Can I undo after saving?** Yes. Saving does not clear the history.

**Why is an old step gone?** The history reached **Undo levels** or its memory limit; the oldest steps are dropped first.

## Related pages

- [Selection tool](selection-tool.md)
- [Clipboard](clipboard.md)
- [Autosave and crash recovery](autosave-crash.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
