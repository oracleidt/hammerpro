[Documentation](../README.md) > Features > Particles | [Français](../../fr/features/particles.md) | [Русский](../../ru/features/particles.md)

# Particles

Hammer Pro has a particle browser that lists every particle system of the mounted particle files (`.pcf`), and a particle preview that plays the effects of `info_particle_system` entities in the 3D views, like Hammer++.

## Particle browser

### Opening it

- **View > Panels > Particle browser** adds the **Particles** tab to the right dock (or removes it). Saved as `show_particle_browser` in `[browser]` (default off).
- The browse button of a particle key (`effect_name` of `info_particle_system`) in the entity panel opens it in pick mode.
- No default shortcut. With the Hammer++ keymap preset (**Settings > Keyboard**), the shortcut is `Shift+F11`.
- The `⊞` button (**Open in a window**) moves the browser into a floating window; `⊟` (**Back to the dock**) puts it back.

### Options

| Control | What it does | Default |
|---|---|---|
| **Grid** / **List** | Thumbnail grid, or one row per system with a small thumbnail, its particle file and its source badge. Kept for the session. | Grid |
| Tile size slider (grid only) | **Tile size** of the particle and model grids, 48 to 256 points by steps of 8. `tile_size` in `[browser]`. | 96 |
| **Search particle systems (!word excludes)** | Keeps the systems whose name or file contains every word; `!word` excludes. | |
| Particle file dropdown | **All particle files**, or the systems of one file. | All |
| Source chips **All**, **Game**, **Addons**, **Workshop** | Keep the systems of one kind of source (a chip appears only when the content has it). | All |
| **Garry's Mod content only** | Only what every Garry's Mod player has. Shared with the model and sound browsers (`gmod_only` in `[browser]`). | Off |
| **Systems: N** | Number of systems listed ("Reading the particle files..." while they load). | |

Thumbnails show the material of the system (a `?` when it has none or it is missing). Hover a system for its name, file, material and source.

### Using a system

- Click selects a system.
- Double click (or `Enter` while the pointer is over the list): in pick mode, writes the system into the key; otherwise sets it on the selected `info_particle_system` entities.
- Footer: the selected name, its file and source, **Apply to selected (N)** (sets the system as `effect_name` of the selected `info_particle_system` entities, one undo step) and **Copy name**.

### Picking for a key

A particle key of the entity panel (see [Entity properties](entity-properties.md)) has a `▾` button that lists the particle systems of the game content with their file (searchable, and you can type a value that is not listed), and a browse button ("Choose in the particle browser (double click a system)").

In pick mode the browser shows "Pick for *key*: double click an item, or select it and press Use." with the current value selected. Double click, `Enter` or **Use** writes the system (one undo step) and returns to the **Entity** tab; **Cancel** leaves pick mode.

## Particle preview in the views

### Turning it on

| Option | Where | What it does | Default |
|---|---|---|---|
| **Show particles** | **View > Display options**, and **Settings > Nav mesh and particles** | Plays the effects of `info_particle_system` entities near the camera in the 3D views. `show_particles` in `[lab]`. | On |
| **Particles of the selection only** | Same places | Plays only the effects of the selected particle systems. `particles_selected_only` in `[lab]`. | Off |
| **Preview volume effects** | Same places | Textured, animated reproduction of `func_smokevolume`, `env_embers`, `func_dustcloud` and `func_dustmotes` (see below). `show_entity_fx` in `[lab]`; action `toggle_entity_fx`. | On |

### What plays

- The definitions come from the particle files of the mounted content: the files of the map's particle manifest (`maps/<map>_particles.txt`) first, then the particle catalog of the content. They are read in the background, with the look of their materials (`$additive`, `$overbrightfactor`, `$addself`, fade sizes, the sprite sheet of the base texture). Child systems are loaded too.
- Every effect plays whatever its start state (`start_active`), at its control points: the entity itself with its angles, and the entities named by its `cpoint1` to `cpoint63` keys. Continuous effects start already running (1.5 seconds ahead).
- To keep the cost low whatever the map size, only effects within 4096 units of the camera play, at most 24 at once (nearest first), with at most 3000 particles in all and 400 per effect.
- Particles are drawn by the renderer with their sprite sheet frame, screen aligned, upright, flat or stretched (trails, ropes), blended or additive. The preview is an approximation: some operators of the particle system are not simulated.
- In the [player view](player-view.md) with the compiled world, the particle systems of the compiled map play instead.

Hover or select an `info_particle_system` in a 3D view to see its effect name above it, with its state: nothing more when it plays, "(not playing: too far or too many)", "(loading...)" or "(not found in the particle files)".

## Volume effects in the views

`func_smokevolume`, `env_embers`, `func_dustcloud` and `func_dustmotes` are brush entities rendered by the game engine. **Preview volume effects** approximates them in the 3D views using their visible brush bounds and mounted materials. A point entity without brushes has no valid volume and is not previewed. The selected entity is highlighted normally.

These four effects need brush bounds in world coordinates. Saving/exporting repairs an old brush-center `origin` without moving its volume; trains and point-entity positions are unaffected. After upgrading an affected map, save it and run a **full geometry compile** once, including when using an external compiler. An entities-only update cannot repair bounds already baked into an old BSP. Effect materials are included in compile staging and packing inputs.

| Entity | Keys used | Sprite | Look |
|---|---|---|---|
| `func_smokevolume` | `Color1`, `Color2` (each puff picks a colour between them), `ParticleDrawWidth` (puff size), `ParticleSpacingDistance` (puffs sit on a grid with this spacing), `Density` (opacity), `MovementSpeed` (sway), `RotationSpeed` (spin), `Material` | `particle/particle_smokegrenade` (or the `Material` key) | Soft puffs on a grid, turning and swaying slowly |
| `env_embers` | `rendercolor`, `renderamt`, `density` (sparks per second), `lifetime`, `speed` | `particle/fire` | Small sparks emitted along the entity angles, fading from size 1 to 0; optional env_wind influence |
| `func_dustcloud` | `Color`, `SizeMin`, `SizeMax` (sprite half size), `SpawnRate`, `LifetimeMin`, `LifetimeMax`, `SpeedMax` (random speed on each axis), `FallSpeed`, `Alpha`, `DistMax` | `particle/sparkles` | Large sparkles with a sine-squared lifetime fade and distance fade; may drift outside the brush |
| `func_dustmotes` | Same keys | `particle/sparkles` | Sparkles whose size scales with view depth using Source mote units |

Defaults follow the FGD. Dust is lit by lightmaps in game and shown unlit in the preview. An unnamed `env_embers` starts enabled; a named one needs **Start On** (flag 1) or runtime I/O. Dust respects `StartDisabled` and `Frozen`; runtime I/O is not simulated. An explicit `Material` name is retained even when missing, so the preview does not hide that problem by selecting a stock replacement. Built-in sprites can use their known fallback. The preview is limited to 24 nearby effects, 2048 particles each (2048 puffs for smoke), and 8192 overall within 3072 units or the smaller `DistMax`. Hidden brushes do not contribute a preview volume. The game uses the original density and spacing, so a very small smoke spacing can be much more expensive than its bounded preview.

### Wind from env_wind

With **Preview volume effects** enabled, `func_dustmotes` and `func_dustcloud` respond to the map's `env_wind` controllers. **Affected By Wind** must be enabled (default on for dust motes), **Frozen** must be off, and **Start Disabled** must be off for the effect to appear. Turning **Affected By Wind** off keeps the entity's own random motion and fall speed.

| env_wind setting | Preview |
|---|---|
| **Angles** | Initial horizontal wind direction; yaw is used |
| **Min/Max normal speed** | Base wind speed between gusts |
| **Min/Max gust speed** | Speed during gusts |
| **Min/Max gust delay** | Random time between gusts |
| **Gust Duration** | Duration of each gust |
| **Max gust dir change** | Direction changes at the start of gusts |
| **Radius** | `-1` affects the whole map; a positive radius affects particles inside that sphere |

Local controllers take priority inside their radius; otherwise the first global controller is used. Hidden controllers are excluded. Editing a controller or the affected entity's options updates the preview directly. Dust accelerates toward the current horizontal wind while retaining its vertical fall speed. This follows the wind-response behavior in the [Source SDK](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/client/c_func_dust.cpp).

Selecting an `env_wind` shows an animated direction arrow and its current speed in the 3D view, labeled **Normal wind** or **Gust**. Existing radius helpers show the controller's area. The preview uses a deterministic gust sequence, not the target game's random sequence, and does not execute entity I/O.

Map ropes (`move_rope` and `keyframe_rope`) also respond while **Show ropes** and **Simulate ropes** are enabled, unless **Disable Wind** is on. Wind simulation is limited to rope nodes within 1000 units of the camera. `env_embers` responds to `env_wind` when `AffectedByWind` is 1; with 0 it keeps only its own movement. `func_smokevolume` keeps its own movement. Particle systems still depend on the operators supported by their preview simulator.

Through MCP, use `get_entity` and `set_entity_keyvalues` to read or change the controller and the `AffectedByWind`, `Frozen` and `NoWind` keys. `set_setting` controls `lab.show_entity_fx`, `helpers.show_ropes` and `helpers.simulate_ropes`.

## Particle preview window

The `▶` button next to the **Particle System Name** field (`effect_name`) of an `info_particle_system` (also **View > Panels > Particle preview...**, action `particle_preview`) opens a window with:

- a search box (all words must appear in the name or file; `!word` excludes) and the list of every system of the content index and the particle files, with its file in the tooltip;
- a live view of the selected system, played from the real particle file with the same simulation, the same renderer and the same sprites as the 3D views (an offscreen render of the renderer shown as an image, at most 400 particles): textured sprites, additive and alpha blending, sprite sheet frames, trails and ropes. Drag to orbit, scroll to zoom, double click to reset the camera. The control points are at the origin with no angles, like an `info_particle_system` without `cpoint` keys. **Pause**/**Play**, **Restart** and **Turn** (the automatic rotation) are under it, with the particle count and the operators that are not simulated. Opening it with the `particle_preview` action while an `info_particle_system` is selected starts on its effect;
- **Assign to N entities** (or **Use for this field** when opened from a field): writes the name into the key (one undo step). A double click on a system does the same. **Copy name** copies it.

Closing the window while it waits for a choice cancels that choice.

## Integrated particle editor

Open **Other > Particle editor** (action `particle_editor`). This editor creates and changes the systems inside a PCF file. The particle browser and its smaller preview window remain available for choosing existing game effects.

### Files and systems

**New** creates an empty document. **Open PCF** reads an external file. The path field accepts an absolute path or a mounted path such as `particles/fire_01.pcf`; **Load** opens it and **Mounted PCFs** lists indexed game files, filtered by the path field. Opening a mounted file does not overwrite the game archive: choose an external path with **Save as**. File and folder dialogs run independently of the editor window.

The left panel filters systems by name. Select a system to show its properties, operators and children. Enter a name and use **Create** or **Rename**; **Duplicate** copies the system and its operator graph under a unique name. **Delete** removes the system and its child references. The document has its own **Undo** and **Redo** history, separate from map editing. Use the title-bar X or the toolbar **Close** button to close the editor. Closing, replacing or clearing an unsaved document opens a foreground modal asking whether to discard changes. Cancel, Escape or clicking outside the prompt returns to editing without losing changes. Choosing **Discard changes** when closing restores the last saved document (or the empty document for a new file), clears the discarded edit history and closes the editor. Quitting Hammer Pro does not ask again for those discarded changes and does not reopen the editor. An asterisk in the window title marks unsaved changes, and undoing back to the saved state clears it.

Saving writes binary DMX version 5. The document retains element identities, element references, operator order, typed attributes, external references and binary attribute payloads rather than rebuilding a file from the subset understood by the preview. Opening and saving a file therefore preserves operators that the preview does not implement. Malformed references, invalid values and files above the PCF size limit are rejected.

### Operators and parameters

Each system has **Emitters**, **Initializers**, **Operators**, **Force generators**, **Constraints** and **Renderers** groups. The operator library supplies common Source functions and their initial parameters. Opening or importing a PCF also adds its operator definitions to the session's library. A function name can be entered directly to add another Source operator.

Select an operator in the tree to edit every stored parameter, including its `functionName`. **Inspect referenced element** follows nested DMX element references into their own properties. **Copy** and **Paste** copy operators across systems. **Move up**, **Move down**, or dragging a row onto another row in the same category changes execution order. Every operation supports undo. Add or remove parameters using their exact Source names; unknown parameters retain their values and types.

| Parameter type | Editor |
|---|---|
| Integer, float, Boolean, string | Numeric drag control, checkbox or text field |
| Color | Color and alpha picker |
| Vector 2, 3 or 4 | Individual numeric components |
| Element reference | Dropdown of document elements, including an empty reference |
| Arrays | Indexed values with insertion and removal |
| Float arrays | Numeric values and a draggable curve |
| Color arrays | Color stops and a gradient preview |
| Angles, quaternion, matrix | Numeric components retaining the original DMX type |
| Other binary types | Hexadecimal payload editor retaining the DMX type |

Edits to numeric and text controls are committed when the gesture or field edit ends, so undo returns to the value before the edit. Save and document replacement commit pending edits before processing the action. Minimum/maximum inconsistencies, invalid control point indices and other problems appear in **Validation**.

The `radius scale` and `alpha fade and decay` operators also have draggable lifetime/value handles that edit their actual Source timing and scale/alpha fields. The `color fade` operator shows a gradient from the system's base color to its fade color. The individual Source parameters remain editable below these controls.

System properties include material, maximum and initial particle counts, radius, color and bounding box. Use **Choose sprite material** beside a material field to search mounted materials and choose from thumbnails. The material's existing Source shader and texture sheet are used for previewing.

**Child systems** links another system in the document with a start delay. Select a child row to edit its reference, delay and end-cap settings or remove it. Cyclic child graphs are rejected. Other control-point and operator parameters are edited through their stored fields.

### Live preview

The right panel uses the same particle simulation and GPU renderer as the map's 3D view. It displays the selected system and its children with mounted sprite materials, alpha/additive blending, animation frames, trails and ropes supported by that renderer.

Drag to orbit, scroll to zoom and double click to restore the camera. **Pause/Play**, **Restart**, playback speed from 0.05x to 4x, **Grid** and the background color control affect only the preview. Add control points, enter their coordinates or drag their CP markers in the canvas. The statistics show the particle count and the measured CPU simulation time. The preview caps active particles at 30000 and displays a warning when the cap is reached.

The PCF editor can preserve and edit more operators than the simulation implements. The preview and validation panel explicitly list functions that are not simulated. Such functions are still saved for Source; a saved effect must be checked in the target game before treating its appearance as final. A model or screen-space renderer is not converted into an approximate sprite renderer.

### Templates, importing and addon export

The **Fire**, **Smoke**, **Sparks**, **Rain** and **Explosion** templates add editable systems to the current document. Repeated imports use unique system names. **Import systems** reads another PCF, copies its systems and remaps their graph references.

**Export addon** chooses an addon root and writes `particles/<file>.pcf` plus `particles/particles_manifest.txt`. Existing manifest entries are retained. The registration window provides `game.AddParticles` and `PrecacheParticleSystem` calls that can be copied into the addon's Lua. Exporting the PCF does not package its sprite materials: those must exist in the game content or be included with the addon.

**Assign to selection** writes the selected system's name into `effect_name` of selected `info_particle_system` entities, or into a waiting particle field. The map change is one map undo step. Save/export the edited PCF and make it available to the game so that the named effect can be loaded there.

### MCP access

| Tool | Purpose |
|---|---|
| `get_particle_document` | Read the current document, systems, typed parameters and editor state |
| `list_particle_operators` | Discover operator categories, function names and default parameters |
| `particle_editor` | Perform file, system, operator, parameter, child, history, preview and assignment operations |
| `capture_particle_preview` | Capture the live editor preview through the shared GPU renderer |

The `particle_editor` operations include `new`, `open`, `read`, `import`, `save`, `export`, `validate`, `create`, `rename`, `duplicate`, `delete`, `select`, `preset`, `add_operator`, `remove_operator`, `move_operator`, `copy_operator`, `paste_operator`, `set_parameter`, `remove_parameter`, `add_child`, `remove_child`, `undo`, `redo`, `preview` and `assign`. They use the same document and undo history as the window. Use the [generated MCP reference](../../MCP.md) for the current argument schema and supported value types. The editor must be open and rendered before capturing its preview.

## Tips

- On a map with many effects, turn on **Particles of the selection only** to watch one effect at a time.
- Through MCP, `toggle_entity_fx` and `particle_preview` (`run_action`), `lab.show_entity_fx` (`set_setting`), `search_particles` searches the systems of the content and `list_particle_systems` reports the state of each `info_particle_system` of the map (see the [MCP tool reference](../../MCP.md)).

## FAQ

**An effect says "not found in the particle files".** No mounted particle file defines that name. Check the spelling, mount the game or addon that has it, or add its file to the map's particle manifest.

**An effect does not play although it is in view.** It is more than 4096 units away, or 24 nearer effects already play. Move the camera closer.

**Why does the browser list nothing?** The game content is not loaded, or the search, file filter, source chips or **Garry's Mod content only** hide everything.

## Related pages

- [Entity properties](entity-properties.md)
- [Render modes and display](render-modes-display.md)
- [Player view](player-view.md)
- [Model browser](model-browser.md)
- [Sound browser](sound-browser.md)
- [Editor performance](../troubleshooting/performance.md)

---
[Back to the documentation index](../README.md)

### Fade playback

Select one `env_fade` and click **Play** above its entity properties. The preview affects the 3D views, with `rendercolor`, `renderamt`, `duration`, `holdtime`, **Fade In** and **Modulate**. Fade In holds the starting color before clearing it; Fade Out reaches the color before holding it. Playback stops automatically after duration plus hold time, including when **Stay Out** is set. **Stop** clears the preview immediately. This does not change the map or trigger outputs. Switching maps or deleting/hiding the entity clears its visible effect.

### Shadow control

A visible `shadow_control` previews projected model shadows in textured and lit 3D views. It uses the entity's angles (or legacy direction), shadow color, distance and `disableallshadows`. No sunlight is required, and it does not change the lighting environment. Hidden models and models with shadows disabled do not cast these shadows. This directional shadow-map preview is an approximation of Source's projected model shadows, rather than its exact render-to-texture implementation.

### Steam jets and accuracy

`env_steam` and legacy `env_steamjet` preview normal steam with Start On, angles, color/alpha, Speed, SpreadSpeed, JetLength, Rate, StartSize, EndSize and RollSpeed. The legacy class uses its sideways orientation. Heat-wave refraction is not reproduced by the sprite preview. Smoke keeps the requested width and opacity and fades near the camera. Cloud and mote sizes follow their different engine units, and both use `particle/sparkles`. Zero emission rates and zero alpha stay invisible. Dust can leave its spawning brush before its lifetime ends. Lighting, collisions, heat distortion and runtime I/O can still differ from a compiled map; the editor is not a substitute for checking the result in game.
