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
