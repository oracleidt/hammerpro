[Documentation](../README.md) > Features > Display colors | [Français](../../fr/features/display-colors.md) | [Русский](../../ru/features/display-colors.md)

# Display colors

Give any brush or entity its own color in the 2D views, so the parts of a busy map are easy to tell apart: the floors in one color, the walls in another, the triggers, the detail and the props each in theirs. The color is saved in the map the way Valve Hammer and Hammer++ save it, so the other editors show the same colors.

## Opening it

| Where | What |
| --- | --- |
| **Properties panel** (right dock), **Display color** section | Shown for every selection: world brushes (under **Tie to entity**), brush entities, point entities, groups and mixed selections (above **Editor comments**). |
| Right click in a 2D or 3D view > **Set display color...** | A small **Display color** window for the current selection. |
| **Edit > Display colors > Set display color...** (`set_display_color`) | The same window. |
| **Edit > Display colors > Random display colors** (`random_display_colors`) | A different color for each selected object, at once. |
| **Edit > Display colors > Reset display colors** (`reset_display_colors`) | The selected objects get their default look back. |
| **View > Display options > Object colors in 2D** (`toggle_object_colors_2d`) | Shows the display colors in the 2D views, or the classic look. |
| **View > Display options > Object colors in 3D wireframe and flat** (`toggle_object_colors_3d`) | Also uses them in the 3D view in wireframe and flat mode. |

None of these actions has a default shortcut; give them one in **Tools > Options... > Keyboard** if you use them a lot.

## The Display color section

| Control | What it does |
| --- | --- |
| Color swatch | Opens a color picker. Dragging in the picker recolors the selection live; the whole drag is one undo step. Next to it: the hex value, **Default** when the objects have no chosen color, or **(multiple values)** when the selected objects have different colors. |
| Palette | 16 preset colors chosen to read well on dark and light 2D backgrounds and to stay clear of the red selection color. Hover a swatch to see its hex value; click it to apply it. |
| **Random color per object** | Each selected brush and each selected entity gets its own color, spread around the color wheel so neighbours differ. |
| **Reset to default** | Back to the usual look: world brushes white, entities in the color of their game data (FGD). Greyed out when nothing has a chosen color. |
| **Turn on** | Shown only when **Object colors in 2D** is off, to switch it back on. |

Every change is one undo step (`Ctrl+Z`).

## What gets colored

- **World brushes**: each selected brush.
- **Brush entities** (`func_detail`, triggers...): the entity and all its brushes, as one object. In **Solids** selection mode you can color a single brush of an entity; that brush then keeps its own color.
- **Point entities**: the box, the model wireframe in the 2D views and the entity name label.
- **Groups**: selecting a group (selection mode **Groups**) colors every member, and the group's own color is updated too when the whole group is selected.
- **Mixed selections**: everything selected, brushes and entities alike.

## How the 2D views pick a color

With **Object colors in 2D** on (the default), each object is drawn in, from first to last:

1. its chosen display color;
2. the color of its visgroup, when **Visgroup colors** is on (see [Visgroups](visgroups.md));
3. the classic color: white for world brushes, the entity's game data color for entities.

The selection is always drawn in the selection color (red by default) on top, and the hovered object is lightened, so the selection stays easy to see whatever colors you pick.

On a light 2D background (**Tools > Options... > Viewports > 2D background**), the 2D views darken light colors (white world brushes included) and strengthen their hue so they keep enough contrast, and draw a softer grid so the objects stand out; on the default dark background the colors are drawn as they are.

With **Object colors in 2D** off, the views show the classic look and ignore the chosen colors (they stay in the map). The 3D view keeps its classic colors unless **Object colors in 3D wireframe and flat** is on; textured and lit modes never change.

## Saved in the map

The color is the standard `color` key of the `editor` block of each solid and entity (and group) in the VMF:

```
editor
{
	"color" "40 165 225"
	"visgroupshown" "1"
	"visgroupautoshown" "1"
}
```

Valve Hammer and Hammer++ read and write this same key, so the colors survive a round trip and show up in their views too. Hammer gives every new brush a random color of the form `0 x y` (x and y from 100 to 255) and every entity `220 30 220`; Hammer Pro does the same and treats those values as "no chosen color", which is why maps from Hammer keep their classic look. A color you pick that happens to match that pattern is nudged by one step (for example `0 180 255` is saved as `1 180 255`), which you cannot see.

## Settings

| Setting | Default | What it does |
| --- | --- | --- |
| `render.object_colors_2d` | `true` | **Use object colors in 2D** (**Tools > Options... > Rendering**). |
| `render.object_colors_3d` | `false` | **Object colors in 3D wireframe and flat**. |
| `edit.new_brush_color` | none | **Color of new brushes** (**Tools > Options... > Editing**, **Custom**): the display color given to the brushes you create with the tools. Off: Hammer's usual random default color. |

## MCP

| Tool | What it does |
| --- | --- |
| `get_display_color` | Colors of objects (default: the selection): `color` per object (`[r, g, b]` or `null` for the default look), the common `state` (`none`, `default`, `{ "custom": [r, g, b] }` or `mixed`) and the two view settings. |
| `set_display_color` | `color: [r, g, b]`, `random: true` or `reset: true` on `ids` / `solid_ids` or the selection. One undo step. |
| `set_setting` | `render.object_colors_2d`, `render.object_colors_3d`, `edit.new_brush_color` (`[r, g, b]` or `null`). |
| `run_action` | `set_display_color`, `random_display_colors`, `reset_display_colors`, `toggle_object_colors_2d`, `toggle_object_colors_3d`. |

See the [MCP tool reference](../../MCP.md).

## Tips

- Color by role rather than by room: one color for detail brushes, one for clips and triggers, one for the structure. The 2D views then read like a plan.
- **Random color per object** on a whole floor at once is a quick way to see where each brush starts and ends.
- Visgroup colors and display colors work together: leave objects without a chosen color and they follow their visgroup.

## FAQ

**I opened a Hammer map and everything is white, as before.** Brushes made in Hammer carry Hammer's automatic colors, which Hammer Pro treats as "no chosen color". Pick colors and they show at once.

**My colors disappeared.** Check **View > Display options > Object colors in 2D**. The colors are still in the map.

**Will Hammer show my colors?** Yes, they are the standard editor colors. Hammer uses them for groups, entities and its 3D wireframe; how much of it shows in its 2D views depends on its own options.

## Related pages

- [Entity properties](entity-properties.md)
- [Visgroups](visgroups.md)
- [Groups](groups.md)
- [Render modes and display options](render-modes-display.md)
