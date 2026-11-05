[Documentation](../README.md) > Features > Multiple maps | [Français](../../fr/features/multiple-maps.md) | [Русский](../../ru/features/multiple-maps.md)

# Multiple maps

Hammer Pro keeps several maps open at once, each in its own tab above the views, with its own cameras, selection, undo history and loaded pointfile. You can also save the arrangement of the views and panels as named layouts.

## Tabs of open maps

The tab bar sits above the views: one tab per open map, a `*` after the name when the map has unsaved changes, and the full path in the tooltip (*Not saved yet* for a new map).

| Gesture | What it does |
| --- | --- |
| Click a tab | Shows that map. |
| Middle click a tab | Closes it. |
| Close button (cross) of a tab | Closes it. |
| **+** at the end of the bar | **New map** in a new tab. |
| Right click a tab | **Close**, **Close other maps** (when several maps are open), **Copy the path** and **Show in the folder** (for a saved map). |

Long names are shortened; the bar scrolls sideways when the tabs do not fit.

## Commands

| Command | Default key | What it does |
| --- | --- | --- |
| **File > New map** | `Ctrl+N` | Opens a new empty map in a new tab. |
| **File > Open map...** | `Ctrl+O` | Opens a map in a new tab (see below). |
| **File > Close** | `Ctrl+F4` | Closes the map shown, after asking to save its changes (*Save the changes to map before closing it?*). Closing the last map leaves a new empty one. |
| **File > Close other maps** | | Closes every other map, asking for each modified one in turn. |
| **Window > Next map** | `Ctrl+Tab` | Shows the next tab. |
| **Window > Previous map** | `Ctrl+Shift+Tab` | Shows the previous tab. |
| **Window > Open maps** | | Lists the open maps (the one shown is checked); click one to show it. |

Quitting Hammer Pro asks about every modified map in turn.

## Opening maps

With **Open maps in new tabs** (**Settings > General**, section **Maps**, key `workflow.open_in_new_tab`, on by default):

- an opened map gets its own tab;
- a new untouched "untitled" map is replaced instead of keeping an empty tab;
- a map that is already open is shown instead of being loaded twice.

Off, an opened map replaces the current one, after asking to save its changes, like Hammer.

Everything that belongs to a map follows its tab: cameras, selection, undo history, pointfile, rotation pivot and the per map data of its sidecar file (release options, ignored problems...). Autosave saves every modified open map, not only the one shown (see [Autosave and crash recovery](autosave-crash.md)).

## Layouts

A layout is the arrangement of the views and panels: the view in each quadrant, the split positions, the maximized view, and which dock panels are shown (textures, entity properties, visgroups, outliner, models, console).

| Command | What it does |
| --- | --- |
| **Window > Layout > Save layout...** | Asks for a name (*Name of the layout:*, `Layout N` proposed) and saves the current arrangement. A layout with the same name is replaced (you are warned). 32 layouts at most. |
| **Window > Layout > Reset layout** | Back to the default arrangement: 3D, top, front and side views, centered splits, no maximized view, every dock panel shown. |
| **Window > Saved layouts** | Click a layout to apply it. **Delete** lists the layouts to delete one. *No saved layout* when the list is empty. |

Layouts are stored in settings.toml (`[workflow.layouts]`), so they are the same for every map.

## MCP

`list_tabs`, `switch_tab`, `close_tab`, `new_map`, `open_map` and `list_recent_maps` work with the open maps from AI apps; `set_view_layout` changes the views of the quadrants. See the [MCP tool reference](../../MCP.md).

## Tips

- Keep a prefab or instance map open in a second tab and copy objects between tabs with `Ctrl+C` and `Ctrl+V`.
- Save a "lighting" layout with a big 3D view and a "brushwork" layout with the four views, and switch from the **Window** menu.
- **Copy the path** in the tab menu is the quickest way to paste the map path in a message or a script.

## FAQ

**Can I open the same map twice?** No. Opening a map that is already open just shows its tab.

**Does each map keep its own undo history?** Yes. `Ctrl+Z` undoes the changes of the map shown only.

**How do I go back to one map at a time, like Hammer?** Turn off **Open maps in new tabs** in **Settings > General**.

## Related pages

- [VMF compatibility](../getting-started/vmf-compatibility.md)
- [Interface tour](../getting-started/interface-tour.md)
- [Clipboard](clipboard.md)
- [Settings window](settings-dialog.md)

---
[Back to the documentation index](../README.md)
