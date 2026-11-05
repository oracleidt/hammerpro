[Documentation](../README.md) > Features > Instances | [Français](../../fr/features/instances.md) | [Русский](../../ru/features/instances.md)

# Instances

A `func_instance` places the content of another VMF file in your map: build a door, a lamp or a whole room once, place it many times, edit the file and every copy follows. Hammer Pro draws the content of each instance, edits instance files, collapses instances into the map, and its compiler merges them like VBSP.

## Opening it

**Tools > Instancing**:

| Action | What it does | Default shortcut |
| --- | --- | --- |
| **Create instance from selection...** | Saves the selection to a new instance file and replaces it with a `func_instance`. | `Alt+S` |
| **Edit instance** | Opens the file of the selected instance. | none |
| **Collapse selected instances** | Replaces the selected instances with their content. | none |
| **Collapse all instances...** | Same for every visible instance of the map, after a confirmation. | none |
| **Instances: hidden** / **Instances: tinted** / **Instances: normal** | How the content of instances is drawn. | none |

The **Instance** tab of the right dock shows the file and the parameters of the selected instance.

## How an instance is merged

The rules are VBSP's, used by the views, by **Collapse** and by the compiler:

- the content is moved by the `origin` and turned by the `angles` of the `func_instance` (brushes with texture lock, entity keys by their FGD type);
- every key value and output value gets the replacements of the instance (`replace01`... keys holding `$variable value`, every occurrence, case insensitive);
- entity names, targets and output targets are fixed up with the instance name: `name-x` (prefix, the default), `x-name` (postfix) or left alone (none). Names starting with `@` (global) or `!` (like `!activator`) are never changed; an unnamed instance uses `InstanceAuto<n>`;
- `func_instance_parms` entities are dropped;
- nested instances are merged too, with a guard against an instance that contains itself, a depth limit and a size limit.

Instance files are looked for from the folder of the map (the `file` key is relative to it), then in the game content (read only).

## Display

| Mode | What you see |
| --- | --- |
| **Instances: hidden** | Only the `func_instance` entity. |
| **Instances: tinted** (default) | The content, tinted, like Hammer. |
| **Instances: normal** | The content drawn like the rest of the map. |

The content of the selected instances is outlined (nested instances included) and their file is named in every view. An instance whose file cannot be found or read gets a red cross and a label at its origin.

## Create instance from selection

`Alt+S` (Hammer++'s key) opens **Create instance from selection**:

| Field | What it does |
| --- | --- |
| **File name:** | Name of the new VMF (no folder separator, no `: * ? " < > \|`). |
| **Folder:** | Where it is saved, with **Browse...**. Defaults to the **Default folder** setting (`instances`, relative to the map folder). An unsaved map needs a full folder path. |
| **Origin:** | X, Y, Z. The selection is saved around this point, which becomes the origin of the `func_instance`. Starts at the center of the selection, snapped to the grid. |
| **Replace the existing file** | Shown when the file exists. |
| Instance file key | The `file` value the instance will get (relative to the map folder when it can be). |

**Create** writes the file in the background (atomic write), then replaces the selection with a `func_instance` of that file in one undo step. The selection is moved by minus the origin (textures locked); groups entirely selected are kept, visgroups are not.

## Edit instance

**Edit instance** (or the **Edit instance** button of the Instance tab) opens the instance file in a new tab, the parent map staying open. When **Open maps in new tabs** is off, it opens in a new Hammer Pro window. With **Show the map around an edited instance** on, the parent map is drawn around the instance, tinted and read only. When you save the instance file, the parent redraws its instances. A file that comes from the game content cannot be edited.

## Collapse

**Collapse selected instances** and **Collapse all instances...** replace instances with their fixed up content (nested instances included). The content joins the visgroups of the instance it replaces, the created objects are selected, and it is one undo step. Instances whose file cannot be found are skipped and reported. **Collapse all instances...** asks first ("Replace the N visible instances of the map with their contents?"), and notes "Objects hidden in the instance files are left out." when that setting is on.

## The Instance tab

With a `func_instance` selected (the first one when several are), the tab shows:

- the file (**File:**), whether it was found, or "Read from the game content (read only)";
- **Edit instance** and **Collapse selected instances** buttons;
- **Instance parameters**: one field per parameter declared by the `func_instance_parms` entity of the file (`parmN` = `$name type [default]`), typed like its FGD type (checkbox, numbers, color, text). A field writes the matching `replaceNN` key (a new value takes the first free slot). **Remove the value** clears it ("Not set: the instance keeps {default}");
- **Other replacements**: the replacements that match no declared parameter, as raw rows, with **Add a replacement ($variable value)**.

The same parameter fields appear under the keyvalues of a `func_instance` in the entity panel.

## Options

**Tools > Options... > Instances** (`[instances]` in `settings.toml`):

| Option | What it does | Default |
| --- | --- | --- |
| **Contents of func_instance** (`display`) | **Hidden**, **Tinted** or **Normal**. | Tinted |
| **Outline the selected instances** (`outline_selected`) | Draws the bounds of the content of the selected instances and the name of their file. | On |
| **Show the map around an edited instance** (`show_context`) | Draws the parent map around an instance opened with **Edit instance**, tinted and read only. | On |
| **Default folder** (`folder`) | Where **Create instance from selection** saves new instances: relative to the map folder, or a full path. | `instances` |
| **Leave out hidden objects of the instance files** (`collapse_skips_hidden`) | Like Hammer++: objects hidden in an instance file are not added to the map when it is collapsed. | On |

## Tips

- Compiles keep `func_instance` entities and the built-in compiler merges them with the same rules, so you do not need to collapse before compiling.
- Use `@` names for entities that must keep their exact name in every copy (a global relay).
- MCP tools: `list_func_instances`, `create_instance`, `set_instance_parameters`, `collapse_instances`, `make_instance`, `get_instance_file` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**An instance shows a red cross.** Its file is missing or unreadable. Check the `file` key: it is relative to the folder of the map.

**Collapse says the instance file is still loading.** Files load in the background; Hammer Pro waits up to 30 seconds, then gives up without changing anything. Try again once the content shows.

**Why can I not edit an instance of the game?** Files read from the game content are read only. Copy the file next to your map and point the `file` key at the copy.

## Related pages

- [Prefabs](prefabs.md)
- [Entity properties](entity-properties.md)
- [Multiple maps](multiple-maps.md)
- [Compiler overview](../compiler/overview.md)

---
[Back to the documentation index](../README.md)
