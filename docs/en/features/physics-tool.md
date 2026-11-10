[Documentation](../README.md) > Features > Physics simulator | [Français](../../fr/features/physics-tool.md) | [Русский](../../ru/features/physics-tool.md)

# Physics simulator

Select brushes, props or ragdolls, then choose **Tools > Physics simulator** or the scales icon in the tool strip. The controls appear in the right panel.

| Control | Effect |
| --- | --- |
| Play / Space | Start or resume the simulation. |
| Pause / Space | Freeze the simulation. |
| Step | Advance one physics step while paused. |
| Keep positions | Apply the current positions and rotations to the map in one undo step. |
| Reset / Escape | Discard the simulation and restore the original map positions. |
| Left mouse in 3D | Grab an object at the point under the cursor; hold to drag, release to let go. |
| Right mouse in 3D | Shoot an object with an impulse at the hit point. |
| Middle mouse in 3D | Create an explosion at the point under the cursor. |

Brushes, displacements and unselected props provide collision surfaces. Models use their `.phy` collision shapes. Ragdolls use their individual physics bodies and joint limits. Water volumes provide buoyancy and drag.

The map stays unchanged during playback. Switching to another editing tool keeps the positions; **Reset** or **Escape** discards them. **Ctrl+Z** during playback cancels the preview; after keeping positions it undoes the complete placement. Editing or switching the map cancels a stale preview.

## Settings

| Setting | Default |
| --- | --- |
| Gravity | 600 Source units/s² |
| Air density | 2 kg/m³ |
| Time scale | 1 |
| Shot speed | 300 units/s |
| Explosion speed | 500 units/s |
| Explosion radius | 256 units |

Settings are saved between sessions. Physics positions are continuous and do not snap to the editing grid. Saved ragdoll poses use the Source `angleOverride` key so they can be preserved in the VMF.

Models without a usable collision file are reported and skipped. Selectable displacements remain collision surfaces rather than moving rigid bodies.

For a quick placement without interactive playback, [Physics placement (drop and settle)](transform-align.md#physics-placement) remains available.

## MCP and scripts

`set_tool_options` with tool `physics` drives the simulator without the mouse (also from `--script` files: `tool physics`, then `tool_option simulate 3`):

| Option | Effect |
| --- | --- |
| `simulate` | Run that many seconds of physics (0.01 to 120, times the time scale) on the selected brushes, props and ragdolls at once, then pause. The first call builds the simulation from the selection; later calls continue it. 0 does nothing (it is the value read back). The map is unchanged. |
| `simulated` | Read only: the seconds simulated so far. |
| `paused` | `off` lets the simulation play in the editor (the tool must be active), `on` freezes it. |
| `keep` | `on` applies the positions, rotations and ragdoll poses to the map in one undo step, like **Keep positions**. |
| `reset` | `on` discards the simulation, like **Reset**. |
| `bodies` | Read only: the number of simulated objects. |

Gravity, air density, time scale and the shot and explosion values are the settings `physics.gravity`, `physics.air_density`, `physics.time_scale`, `physics.shoot_speed`, `physics.explosion_speed` and `physics.explosion_radius` (`set_setting`). Example: select a few props, then `set_tool_options` `{"tool": "physics", "options": {"simulate": 3}}` and `{"tool": "physics", "options": {"keep": true}}` drops them on the floor in one undo step.
