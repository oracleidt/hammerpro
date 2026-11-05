[Documentation](../README.md) > Features > I/O and logical view | [Français](../../fr/features/io-logical-view.md) | [Русский](../../ru/features/io-logical-view.md)

# I/O and logical view

Outputs and inputs wire entities together: "when this fires, send that input to those entities". Hammer Pro edits them in the **Outputs** and **Inputs** tabs of the entity panel (Hammer's I/O editor), draws them as lines in the views, and shows the whole map's logic as a schematic in the logical view.

## Opening it

| What | How |
|---|---|
| Outputs and inputs tabs | **Edit > Properties** (`Alt+Enter`), then the **Outputs (N)** and **Inputs (N)** tabs of the **Entity** tab. |
| Connection lines in the views | **View > Display options > Show connections** cycles the mode; **Settings > Helpers > Input/output connections** sets it. |
| Logical view window | **View > Logical view** (opens and closes it). |
| 2D logical view in a quadrant | **View > 2D logical view** sets the view under the mouse (else the last one used), or choose **2D Logical** in the view type menu of a view header. |
| Entity gallery | **Map > Insert entity gallery**. |

None of these has a default shortcut; you can bind them in **Settings > Keyboard** (see [Keyboard shortcuts](keyboard-shortcuts.md)).

## Outputs tab

The tab lists the outputs of the selected entities. With several entities selected, only the outputs they all share are listed (edits apply to the matching output of each entity), and a note gives the number of **Hidden outputs (not shared by every selected entity)**. The world has no outputs here.

### Toolbar

| Button | Key | What it does |
|---|---|---|
| **+ Add** | `Insert` | Adds an output to every selected entity and selects it. The new output is `OnTrigger` when the class has it, else the first output of the class, else `OnUser1`; it fires always. |
| **Copy** | `Ctrl+C` | Copies the selected outputs (also to the system clipboard as `"Output" "value"` lines). |
| **Paste** | `Ctrl+V` | Pastes the copied outputs on every selected entity. |
| **Delete** | `Delete` | Deletes the selected outputs. |
| (cut) | `Ctrl+X` | Copies then deletes the selected outputs. |
| (select all) | `Ctrl+A` | Selects every output row. |
| **Mark** | | Selects the entities targeted by the selected outputs (names, `*` wildcards, class names) and centers the views on them. |

The keys are your application shortcuts for Copy, Cut, Paste, Delete and Select all. They act on the outputs while the mouse pointer is over the tab and no text field has the focus; elsewhere they act on the map as usual.

On the right, **Invalid: N** counts the outputs with a problem.

### The table

Columns: a status column (**!** for a problem, else the refire count of an output limited in number), **Output**, **Target**, **Input**, **Parameter**, **Delay**. A problem is shown in red in its cell:

- **This output does not exist for this class**;
- **No entity has this target name**;
- **The target does not accept this input**.

Hover a row for the full connection, its parameter, delay, refires and problems. Click selects a row, `Ctrl+click` toggles one, `Shift+click` selects a range.

### Editing the selected outputs

Under the table, the editor changes every selected output at once (**(multiple values)** when they differ). **Select an output to edit it (Ctrl and Shift select several).**

| Field | What it does |
|---|---|
| **My output named** | Picker of the outputs of the class (every output when the class is unknown). |
| **Target entities named** | Text with completion of the names of the map and of the special targets (`!self`, `!activator`, `!caller`, `!player`, `!pvsplayer`, `!speechtarget`, `!picker`). Red when no entity has the name. `Tab` completes, arrows pick, `Enter` accepts. |
| **Via this input** | Picker of the inputs of the classes the target resolves to (every input when it resolves to nothing known). |
| **With a parameter override of** | Free text, **<none>** when empty. Double quotes are refused; when the input is `RunScriptCode`, **Use backticks** replaces them with backticks. |
| **After a delay in seconds of** | 0 to 3600 seconds. Drag or type. |
| **Fire this many times** | **Always** (-1) or a number (Hammer++ refires). Drag or type a number; 0 is skipped, since it would never fire. |

## Inputs tab

It lists the outputs of other entities (and of the world) that target the selected entities: **Outputs targeting this entity: N**, with the columns **Source**, **Output**, **Input**, **Parameter**, **Delay**. An entity without a name cannot be targeted: the tab says so.

Double click a row, or select it and click **Go to source**, to select the source entity and center the views on it.

## ESC delimiter

Inside the map file, the fields of an output are separated by commas (old Hammer) or by the ESC character (newer Hammer builds and Hammer++). With ESC, a parameter can contain commas. Garry's Mod reads both.

**Settings > Entities and browsers > New outputs use the ESC separator** (`[entities] io_esc_delimiter`, on by default) chooses the separator of outputs added from the **Outputs** tab. Existing outputs keep theirs. Outputs added from the logical view use commas.

## Connection lines in the views

Every output can be drawn as a line from its entity to each target, with an arrow at the target, in the 2D and 3D views.

| Mode | Settings label | What is drawn |
|---|---|---|
| Off | **Hidden** | Nothing. |
| Selected (default) | **Selected entities** | The outputs of the selected entities and the outputs that target them. |
| All | **Every entity** | Every output of the map. |

**View > Display options > Show connections** cycles Off, Selected, All, and a notification tells the new mode. The mode is saved as `[helpers] connections`.

Colors (the legend is in **Settings > Helpers**):

- green: **Valid**;
- red: **Missing target**, drawn as a cross at the source;
- yellow: **Output unknown to the FGD**.

Special targets (`!activator`, `!player`...) are resolved at run time, so they get no line.

## Logical view

The logical view shows the entities of the map and their outputs as a schematic. It exists as a floating window (**View > Logical view**, titled **Logical view**) and as a view type of the four views (**2D Logical**, like Hammer). Each quadrant showing it has its own pan, zoom, search and filter. The selection is the map's selection, so it follows the other views both ways.

### What is shown

- Nodes: every entity of the map by default, like Hammer: point entities (`light`, `light_spot`, `light_environment`, `info_` entities, logic...) and brush entities, named or not. A node shows the editor icon of its class (the FGD `iconsprite()` or `sprite()` helper, the icon of the 2D and 3D views), the name (**(unnamed)** when there is none) and the class, with an accent color by class family (logic and filters, triggers, `func_`, lights and environment, props and NPCs, info and path entities, others). Entities hidden by a visgroup are drawn faded.
- Edges: one per output, colored like the connection lines (green valid, red missing target with a cross and the name, yellow output or input unknown to the FGD). An output to its own entity loops above the node.
- Hover an edge for `Output > target.Input (parameter) after N s (once)` and, for a problem, the reason.
- Zoomed out, text and ports are left out and edges are straight, so large maps stay fluid.

### Toolbar

| Control | What it does |
|---|---|
| **Find an entity...** | Shows only the nodes whose name or class contains the text and frames them while you type. `Enter` frames the results. |
| Filter | **All entities** (default), **Named and connected** (entities with a name or a connection) or **Connected only** (entities with an outgoing or incoming connection). Each view keeps its own filter. |
| **Frame all** | Shows every entity (`Home` with the mouse over the view). |
| **Frame selection** | Frames the search results, else the selected entities. |
| **Arrange** | Places every entity automatically and stores the positions in the map (one undo step, "Arrange logical view"). |

The status line gives **Entities: N, outputs: N**, the number of search results, **Outputs with a problem: N**, and the color legend.

### Mouse

| Action | Result |
|---|---|
| Click a node | Selects it (`Ctrl` toggles, `Shift` adds). |
| Click an edge | Selects its source entity. |
| Click empty space | Clears the selection. |
| Drag on empty space | Box selection (`Ctrl` or `Shift` adds). |
| Drag a node | Moves the selected nodes; one undo step ("Move in logical view") on release. |
| Drag from the output port of a node onto another node | Opens the **New output** form. |
| Double click a node | Selects it and opens its properties. |
| Middle or right drag | Pans. |
| Wheel | Zooms around the cursor. |
| `Escape` | Cancels the drag in progress. |

Hidden entities are never selected from the logical view.

Right click menu: **Properties**, **Center the views on it**, and **Use automatic position** (for selected nodes that have a stored position).

### Positions

Positions are stored in the `logicalpos` key of each entity, like Hammer, so they survive in the VMF. Nodes without a stored position are placed by an automatic layered layout (sources on the left, each output one column further right), recomputed only when the connections change. Moving a node stores its position; **Use automatic position** removes it.

### New output form

Dragging from an output port to another node opens **New output**:

| Field | Content |
|---|---|
| **Output** | An output of the source class, typed or picked. The first proposal is an output of the class itself, not of the common bases (`OnUser1`...); `OnTrigger` when the class is unknown. |
| **Target** | The target name. When the target has no name, a field proposes `class_id`: **The target has no name: it gets this one**. |
| **Input** | An input of the target class, typed or picked. |
| **Parameter** | Free text. |
| **Delay** | 0 to 3600 seconds. |
| **Only once** | Fires once instead of always. |

**Add** (enabled when output, input and target name are set) adds the output, naming the target first if needed, in one undo step. **Cancel** or `Escape` closes the form.

## Entity gallery

**Map > Insert entity gallery** places one entity of every point class of the FGD on a grid at the center of the top view, each with the default keyvalues of its class, in one undo step. Spacing follows the largest entity size (128 to 512 units). Use it to check icons, models and helpers at a glance, then undo. Without an FGD, it says **No entity definitions are loaded: the gallery needs the FGD**.

## Tips

- The outputs tab title turns highlighted when an output has a problem: look for the red cells.
- Use **Arrange** once on an imported map, then move the nodes you care about: the others follow the automatic layout.
- Through MCP: `list_connections`, `add_connection`, `edit_connection`, `remove_connections`, `get_io_graph`, `list_targetnames`, `rename_targetname`, `check_entity`, `get_logical_graph` (the nodes, icons and edges of the logical view, with its filter), and `get_views` (a quadrant of kind `logical`). See the [MCP tool reference](../../MCP.md).

## FAQ

**Why does the Outputs tab show fewer outputs than I expect?** Several entities are selected and only the outputs they all share are listed. Select one entity to see all its outputs.

**Why is my output red although the target exists?** Check which cell is red: the output may not exist for the source class, or the input may not exist for the target class in your FGD.

**Do the logical view positions change my map in game?** No. `logicalpos` is an editor key, like in Hammer.

**My connection lines disappeared.** **Show connections** cycles through three modes: run it again until the notification says the mode you want.

**Should I keep the ESC separator on?** Yes for Garry's Mod: it reads both, and ESC lets parameters contain commas. Turn it off only for a tool that cannot read ESC.

## Related pages

- [Entity properties](entity-properties.md)
- [Path tool](path-tool.md)
- [Player view](player-view.md)
- [Render modes and display options](render-modes-display.md)
- [Check for problems](check-problems.md)
- [Camera and navigation](camera-navigation.md)
- [Settings reference](../reference/settings.md)
- [MCP server](../reference/mcp-server.md)

---
[Back to the documentation index](../README.md)
