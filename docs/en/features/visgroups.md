[Documentation](../README.md) > Features > Visgroups | [Français](../../fr/features/visgroups.md)

# Visgroups

Visgroups are named sets of objects you show and hide together: the roof, the detail props, the triggers of an area. Hammer Pro has your own visgroups (saved in the VMF, compatible with Hammer and Hammer++) and auto visgroups built from what the objects are (world geometry, triggers, lights, tool brushes...).

## Opening it

- The **Visgroups** tab of the right dock, shown or hidden with **View > Panels > Visgroups** (shown by default, `ui.show_visgroups`).
- **View > Move to visgroup...** opens Hammer's "Move to visgroup" window for the selection.
- **View > Make a visgroup of hidden objects** turns the objects hidden with **Hide selected** into a visgroup.
- The **Visgroups** tab of the entity panel lists the visgroups of the selected entity (see [Entity properties](entity-properties.md)).

The tab has three sections: your visgroups, the **Cordons** (see [Cordons](cordons.md)) and the **Auto visgroups**.

## Your visgroups

### Toolbar of the tab

| Button | What it does |
| --- | --- |
| **+** | **New visgroup from the selection** (or **New empty visgroup** when nothing is selected). The new visgroup gets a unique name and starts in rename mode. |
| Eye | **Show all visgroups**. |
| Down arrow | **Expand all**. |
| Up arrow | **Collapse all**. |
| Folder | **Move to visgroup...** (needs a selection). |
| Count | Number of visgroups, nested ones included. |

### A row

| Element | What it does |
| --- | --- |
| Arrow | Expands or collapses the children. |
| Checkbox | Shows or hides the visgroup and its children. It is mixed when some child is hidden; a click on a mixed or hidden visgroup shows it fully. |
| Color swatch | Click to change the **Visgroup color**. Consecutive changes of the picker merge into one undo step. |
| Name | Double click to rename (`Enter` commits, `Escape` cancels). |
| Count | Objects directly in the visgroup. The tooltip also gives the total with its children. |
| Click | Highlights the row. |
| Drag onto another visgroup | Makes it a child of that visgroup. Drop below the rows ("Drop here to move to the top level") to move it to the top level. A visgroup cannot be moved into its own children. |

### Right-click menu of a row

| Entry | What it does |
| --- | --- |
| **Select members** | Selects the visible objects of the visgroup and its children. Hidden members are not selected, like in Hammer. |
| **Add the selection** | Adds the selected objects to the visgroup. |
| **Remove the selection** | Removes the selected objects from it. |
| **Move the selection here** | Puts the selection in this visgroup and removes it from its other visgroups. |
| **Move up** / **Move down** | Reorders the visgroup among its siblings. |
| **New child visgroup** | Creates an empty visgroup under this one. |
| **Rename** | Renames it. |
| **Delete visgroup** | Deletes it. Its children move up to its parent and its objects stay in the map: they only leave the visgroup. |

Every change (create, rename, color, move, show or hide, membership) is one undo step.

## Move to visgroup

**View > Move to visgroup...** (or the folder button of the tab) opens a window: "Move N selected objects to:" with the tree of your visgroups and **New visgroup...** (with a **Name:** field). Options:

| Option | What it does |
| --- | --- |
| **Remove from the other visgroups** | The objects leave every other visgroup. |
| **Hide the visgroup** | Hides the visgroup afterwards (show it again with its checkbox). The objects are then deselected. |

**Move** applies it as one undo step. The defaults of both options come from the settings below.

## Visgroup of hidden objects

**View > Make a visgroup of hidden objects** (Hammer++'s "Convert QuickHide objects to visgroup") moves every object hidden with **Hide selected** (`H`) into a new visgroup named `_FromQuickHide(N)`, clears their hidden flag and hides the visgroup instead. The objects stay hidden, but you can now show them again by name. One undo step. The **To visgroup** button of the **Hidden objects** row (auto visgroups) does the same.

## Auto visgroups

Auto visgroups are computed from the map, nothing is stored: each row has a checkbox and a count. Groups with members can be expanded. Children without members are not listed.

| Group | Children |
| --- | --- |
| **World geometry** (solids that belong to no entity) | **Sky**, **Displacements**, **Water**, **Brushes with nodraw faces** |
| **World details** (func_detail and props) | **func_detail**, **Props** |
| **Entities** | **Point entities**, **Brush entities**, **Triggers**, **Lights**, **NPCs**, **Sprites**, **Cubemaps**, **Overlays and decals**, **Instances** |
| **Tool brushes** (every face uses a tool texture) | **Clips**, **Hint and skip**, **Areaportals**, **Occluders**, **Invisible**, **Ladders**, **Other tool textures** |
| **Hidden objects** | The objects hidden with **Hide selected**, with **Unhide all** and **To visgroup** buttons. |

A second list, **From the entity definitions (FGD)**, shows the `@AutoVisGroup` groups of the loaded FGD files with their entity counts ("Entity definitions not loaded: FGD groups unavailable" when no FGD is loaded). The eye button of the section header (**Show every object hidden by an auto visgroup**) shows them all again. Counts refresh a moment after the map changes.

## Visgroup colors

Each new visgroup gets a pleasant color. With **Tools > Options... > Rendering > Visgroup colors** on (`render.use_visgroup_colors`, off by default), brushes are drawn in the 2D views in the color of their visgroup. A display color chosen for an object comes first (see [Display colors](display-colors.md)).

## Options

**Tools > Options... > Cordons and visgroups**, section **Move to visgroup** (`[visibility]` in `settings.toml`):

| Option | What it does | Default |
| --- | --- | --- |
| **Remove the objects from their other visgroups** (`move_remove_others`) | Default of **Remove from the other visgroups**. | On |
| **Hide the visgroup afterwards** (`move_hide`) | Default of **Hide the visgroup**. | Off |

## Tips

- Objects hidden by a visgroup are left out of the compile unless **Compile hidden objects** is checked, like in Hammer (see [Compile and run window](../compiler/compile-window.md)).
- Hidden objects cannot be selected: show the visgroup first.
- MCP tools: `list_visgroups`, `create_visgroup`, `edit_visgroup`, `delete_visgroup`, `set_visgroup_members`, `move_to_visgroup`, `set_visgroup_visible`, `hidden_to_visgroup`, `select_group` (see the [MCP tool reference](../../MCP.md)).

## FAQ

**I clicked Select members and nothing was selected.** Every member is hidden. Show the visgroup with its checkbox first.

**Does deleting a visgroup delete its objects?** No. The objects only leave it; its child visgroups move up one level.

**Are auto visgroups saved in the map?** The groups are computed from the map each time. What they hide is stored on each object in Hammer's `visgroupautoshown` key, so hidden auto visgroups stay hidden after a save. Showing or hiding an auto visgroup is an undo step.

**Will Hammer read my visgroups?** Yes, they are Hammer's own `visgroups` blocks and `visgroupid` keys.

## Related pages

- [Hide and isolate](hide-isolate.md)
- [Cordons](cordons.md)
- [Groups](groups.md)
- [Outliner](outliner.md)
- [Settings reference](../reference/settings.md)

---
[Back to the documentation index](../README.md)
