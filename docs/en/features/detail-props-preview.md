[Documentation](../README.md) > Features > Detail props preview | [Français](../../fr/features/detail-props-preview.md) | [Русский](../../ru/features/detail-props-preview.md)

# Detail props preview

Materials with the `%detailtype` key (the grass textures of Garry's Mod and the like) get detail objects when the map is compiled: grass sprites, small models. This preview scatters the same objects in the 3D view while you edit, so you see the grass on each texture without compiling.

## Turning it on

- The **Detail props preview (grass)** button in the toolbar (leaf icon), next to the fog buttons.
- **Settings > Rendering > Detail props preview (grass)**, setting `render.detail_preview` (off by default).
- The `toggle_detail_preview` action, or the MCP tool `detail_preview` with `enabled` true or false. `detail_preview` without arguments only reports.

## What is drawn

- Every displacement and every brush face (world brushes and `func_detail`) whose material has `%detailtype`, when the map's `detail.vbsp` defines a group of that name. The file is the `detailvbsp` property of the worldspawn (Map properties), else `detail.vbsp`, else `scripts/detail.vbsp` of the game content.
- The placement follows VBSP: the density of the group per surface area, the vertex alpha painted on a displacement choosing between the groups by their `alpha`, `minAngle` and `maxAngle` against the slope of the surface, `upright`, `amount` weights. The same surface always gets the same objects.
- Sprite entries are billboards cut from the sprite sheet of the worldspawn `detailmaterial` (default `detail/detailsprites`) with their rectangle, `spritesize` and `spriterandomscale`; `sprite_shape` cross and tri are drawn as two and three crossing quads; `sway` makes the top of the sprite sway in time (it follows the **Animate materials** option); `detailOrientation` is honored. Model entries are drawn as models.
- Only the surfaces around the 3D camera are filled, nearest first, up to the view distance of the game (1200 units fading over the last 400, or the `fademindist` and `fademaxdist` of an `env_detail_controller`), at most 60000 sprites and 1500 models. The objects are generated a few surfaces per frame, so the camera stays smooth, and drawn instanced.
- The player view draws the detail props of the compiled map and hides this preview.

## Reading the state

The MCP tool `detail_preview` returns the file read and its number of types, the sprite material, how many surfaces use a `%detailtype`, how many are in range, the sprite and model counts, whether the limits cut some, and per material the detail type it names (`known` is false when the file has no such group).

See also [Render modes and display options](render-modes-display.md), [Scatter by material](scatter-by-material.md).
