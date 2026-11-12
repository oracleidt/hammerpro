# Extending Hammer Pro

Feature packages are built in parallel. The rule for every package: work in
your own files. When a host file belongs to another package (or to nobody),
plug in through the hooks described here instead of editing it. Each new
file names its owner package in its header comment.

The foundation work was split in two halves, and each half documents what it
built:

- Registries of the app shell (actions, settings, dispatch, menus, settings
  pages, status bar, sidecar, `hp-app` strings): sections written by the
  other foundation engineer.
- Overlay layers, scene producers, tool hooks, picking, editor extension,
  compile and launch: the sections below.

Package keys used in file names: shell, selection (1), brush (2), workflow
(3), instances (4), visibility (5, cordons and visgroups), helpers (6,
overlays, entity helpers, connections), mesh (7), entities (8), render (9),
lighting (10), placement (11, prefabs and sprinkle), compile (12), startup
(13), launch (14), content (15), health (16), lab (17), mcp (the MCP
server, see [MCP.md](MCP.md)).

## Overlay layers

Geometry drawn in every view whatever the active tool (leak path, cordons,
helpers, entity names, portals...) is an overlay layer.

Files, in `crates/hp-ui-viewports/src/layers/`:

| File | Owner |
|---|---|
| `mod.rs` | trait `OverlayLayer` and `default_layers()` (foundation) |
| `pointfile.rs` | compile (12): the leak path (moved from `overlay.rs`) |
| `cordon.rs` | visibility (5) |
| `portals.rs` | health (16) |
| `instances.rs` | instances (4) |
| `helpers.rs`, `labels.rs` | helpers (6) |
| `render.rs` | render (9) |

Every file has a function `pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>>`
already collected by `default_layers()` (drawing order: pointfile, cordon,
portals, instances, helpers, labels, render), and its own translation table
`pub(crate) const TR: TrTable` (keys start with `viewport.`, registered by
`hp_ui_viewports::register_translations` through `layers::TABLES`). Return
your layers from `layers()` and put your strings in `TR`; you never edit
`mod.rs`, `lib.rs` or `tr.rs`.

The trait (`hp_ui_viewports::layers::OverlayLayer`), every method but `name`
has a default:

```rust
pub trait OverlayLayer {
    fn name(&self) -> &'static str;                                     // unique
    fn prepare(&mut self, state: &AppState) {}                          // once per frame
    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {} // GPU lines, triangles, points
    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {} // egui labels
    fn retained_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> { None }
    fn retained(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {}
}
```

Per frame: `prepare` once, then for each visible view `add` (after the
previous layers, before the 3D grid, the drop preview and the tool overlay)
and `paint` (after the axis gizmo and the FPS label, before the mouselook
hint). Layers only read `AppState`; they decide themselves when to draw (own
view setting in `state.settings`, selection, a loaded file). Keep caches in
`prepare` (it gets `&mut self`) or in a `Cell`/`RefCell`.

Large geometry that rarely changes (thousands of portal edges, every helper
in "All" mode) goes in the retained overlay: return a version from
`retained_version` and fill `out` in `retained`. The viewports refill the
view's retained overlay (the tool's retained overlay plus every layer's)
only when one version, the tool or the view kind changes, and the renderer
draws the uploaded copy every frame. Change the version whenever the content
would differ for this view, and include the view kind when 2D and 3D differ.

Drawing helpers: `crate::overlay::{thick_line_on_top, box_lines_on_top,
face_highlight, cross_marker, pixel_world_size, screen_axes}` and
`crate::paint::{text_pill, overlay_color, TEXT, TEXT_WEAK}`.

Example (`layers/cordon.rs`):

```rust
use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_render::Overlay;

use super::OverlayLayer;
use crate::overlay::box_lines_on_top;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(CordonLayer)]
}

struct CordonLayer;

impl OverlayLayer for CordonLayer {
    fn name(&self) -> &'static str {
        "cordons"
    }

    fn add(&self, state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        let cordons = state.doc.scene().cordons();
        if !cordons.active {
            return;
        }
        for c in cordons.cordons.iter().filter(|c| c.active) {
            box_lines_on_top(out, c.mins, c.maxs, [1.0, 0.1, 0.1, 1.0]);
        }
    }
}
```

A crate other than `hp-ui-viewports` adds a layer through the binary:
`viewports.add_layer(Box::new(MyLayer))` (drawn after the built-in ones);
`Viewports::layer_names()` lists them.

Tests: `crates/hp-ui-viewports/src/tests_hooks.rs` shows a counting layer
driven through the headless harness.

## Scene producers

Geometry derived from the document (decals projected on brushes, overlays,
helper meshes, connections, ropes, instance contents, detail sprites) is
built by a scene producer and synced to the renderer by a registry, so no
package edits `scene.rs`.

Files, in `crates/hp-assets/src/`:

| File | Content / owner |
|---|---|
| `producer.rs` | trait `DerivedProducer`, `DerivedObject`, `DerivedBuild`, `ProducerChanges`, `ProducerCx`, the registry (foundation) |
| `scene.rs` | `producers()`: decals first, then `helpers::producers()`, `instances::producers()`, `render::producers()`, `lights::producers()` (render, 9 owns `scene.rs`) |
| `decals.rs` | `DecalProducer`, the first producer: read it as the reference implementation |
| `helpers.rs` | helpers (6): overlays, helper geometry, connections, ropes, visualizers |
| `instances.rs` | instances (4): instance cache and contents |
| `render.rs` | render (9): detail sprites, sky data |
| `lights.rs` | lighting (10): `parse_light`, `parse_light_value`, `light_direction` (moved from `entities.rs`); the texlight producer (`lights.rad` and the map's `.rad` light the Lit mode, with VRAD's falloff) |

A package needing several files declares them as submodules of its own file
(`mod overlays;` in `helpers.rs` is `src/helpers/overlays.rs`) and adds their
producers to its `producers()` function.

The trait (crate private):

```rust
pub(crate) trait DerivedProducer {
    fn name(&self) -> &'static str;
    /// Remember what to rebuild; true when `build` must run this frame.
    fn changed(&mut self, changes: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool;
    /// Objects to add, replace or remove (by key), plus optional lists.
    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild;
    /// Own render ids (decals only); None allocates one.
    fn object_id(&self, key: u64) -> Option<u32> { None }
}
```

When it runs: every `sync_scene`, after brushes and entities are up to date
(the entity display states in `cx.assets.scene.ents` are current), and, on a
full rebuild, before the material garbage collection so the materials you
use stay alive.

- `ProducerChanges { doc: &Changes, full, entities }`: `doc` holds the ids of
  changed and removed solids and entities and the flags
  (`selection_changed`, `visibility_changed`, `world_changed`,
  `extra_blocks_changed`...). `full`: the renderer was cleared, drop your
  state and rebuild everything (your old objects are already gone).
  `entities`: the FGD or the models/helpers display options changed,
  rebuild every entity based object.
- `ProducerCx { doc, assets, renderer, opts }`: `assets` is the
  `AssetManager` (`asset_material(name, MatKind::World)`,
  `material_info_impl(name)` which returns `None` while loading,
  `model_data(path)`, `fgd`); `renderer` for models and reading flags; `opts`
  the scene options. Do not add objects to the renderer yourself.
- `DerivedObject { key, mesh: ObjectMesh, owner: Option<Id>, flags }`: `key`
  is stable for your object (an entity or solid id). With an owner entity
  the object is selected and hidden with it (flags refreshed on selection,
  visibility and owner changes); `flags` are added (for example
  `ALWAYS_EDGES` once the render package adds it).
- `DerivedBuild { clear, upsert, remove, sprites, models, lights }`: objects
  by key, and `Some(list)` replaces this producer's sprites, model
  instances or preview lights (drawn with the entity ones).
- Sprites that change every frame and do not come from the document
  skip the producers: `gfx.renderer.set_sprite_layer(layer, &sprites)`
  with your own layer id, materials from
  `AssetsApi::sprite_material_id(name, additive)`; an empty list removes
  the layer. Layers are drawn in the 3D views (with the entity sprites:
  hidden with the helpers) and belong to the map shown (parked with it in
  another tab).
- Particles (the particle preview of the lab package) have their own pass:
  `gfx.renderer.set_particles(&[ParticleInstance])`, far to near, each with
  a texture rectangle (sprite sheet frame), an orientation
  (`ParticleOrient`: screen, upright, flat, stretched along an axis), a
  roll, a brightness and additive or blended drawing. They are drawn in
  the 3D views whatever the helpers, never in the 3D skybox pass or the
  cubemap probes, times the exposure of the player view's HDR lighting.
- Render ids come from a reserved range (`0x7000_0000..0x8000_0000`) and are
  reused after removal.
- Waiting for an asset: return `true` from `changed` while something is
  pending (decals do it while the material size loads).

Example (`helpers.rs`):

```rust
use ahash::AHashSet;
use hp_editor::Id;
use hp_render::ObjectFlags;

use crate::producer::{DerivedBuild, DerivedObject, DerivedProducer, ProducerChanges, ProducerCx};

pub(crate) fn producers() -> Vec<Box<dyn DerivedProducer>> {
    vec![Box::new(OverlayProducer::default())]
}

#[derive(Default)]
struct OverlayProducer {
    dirty: AHashSet<Id>,
}

impl DerivedProducer for OverlayProducer {
    fn name(&self) -> &'static str {
        "overlays"
    }

    fn changed(&mut self, ch: &ProducerChanges<'_>, cx: &ProducerCx<'_>) -> bool {
        if ch.full || ch.entities {
            self.dirty.extend(cx.doc.scene().entities().keys().copied());
        } else {
            self.dirty.extend(ch.doc.entities_changed.iter().chain(&ch.doc.entities_removed).copied());
        }
        !self.dirty.is_empty()
    }

    fn build(&mut self, cx: &mut ProducerCx<'_>) -> DerivedBuild {
        let mut out = DerivedBuild::default();
        for id in self.dirty.drain() {
            // `overlay_mesh` (a function of this file) returns None for
            // removed entities and entities that are not overlays.
            match overlay_mesh(cx, id) {
                Some(mesh) => out.upsert.push(DerivedObject {
                    key: u64::from(id),
                    mesh,
                    owner: Some(id),
                    flags: ObjectFlags::empty(),
                }),
                None => out.remove.push(u64::from(id)),
            }
        }
        out
    }
}
```

Tests: pure geometry is unit tested in your file; the GPU test
`crates/hp-assets/tests/gpu.rs` shows how a producer is checked through
`sync_scene` (decal section).

## Tool hooks

Tools implement `hp_app::Tool` (`crates/hp-app/src/tool.rs`). The hooks
below are routed by `crates/hp-ui-viewports/src/lib.rs`; every one has a
no-op default.

- `wheel(&mut self, state, view, notches, modifiers) -> ToolFlow`: called
  for the mouse wheel over a view before the camera, when no camera gesture
  (right button look, pan) or mouselook runs and no menu is open. Return
  `ToolFlow::Handled` to skip the camera zoom (2D) or dolly (3D).
  `notches` is positive when the wheel is pushed away.
- `context_entries(&self, state) -> Vec<ToolMenuEntry>`: shown at the top
  of the viewport context menu (then a separator, then the standard
  entries). `ToolMenuEntry { label (translated), id, enabled }`. The menu
  asks every frame while it is open.
- `context_command(&mut self, state, id)`: the entry with this `id` was
  clicked (the menu closes).
- `click_while_flying(&mut self, state, view, pos, modifiers) -> ToolFlow`:
  a left press in the 3D view during persistent mouselook (Z), `pos` being
  the crosshair at the view center (Hammer: the Selection and Camera tools
  select there, Ctrl + click adds or removes). Not called for a tool that
  works while navigating (the displacement brush keeps its view input).
- Middle click: a middle press and release without moving (less than 4
  points, decided by egui) reaches `view_input` with
  `input.response.middle_clicked()` true and `input.navigating` false. A
  middle drag pans and never reaches the tool.
- Unchanged: `reactivate`, `retained_overlay_version` / `retained_overlay`,
  `overlay`, `paint`, `cursor`, `status_hint`, `key_input` (called by the
  binary after the UI pass).

While a pick request is pending (next section) the tool gets no
`view_input` and no `cursor` call.

```rust
// Illustration: `scale_props`, the `tool.move_to_floor` key and
// `Action::MoveToFloor` are added by the selection package.
impl Tool for SelectTool {
    fn wheel(&mut self, state: &mut AppState, view: &ViewInfo, notches: f32, mods: egui::Modifiers) -> ToolFlow {
        if !mods.alt || state.doc.selection().is_empty() {
            return ToolFlow::Ignored;
        }
        self.scale_props(state, 1.1f64.powf(f64::from(notches)));
        ToolFlow::Handled
    }

    fn context_entries(&self, state: &AppState) -> Vec<ToolMenuEntry> {
        vec![ToolMenuEntry { label: t!("tool.move_to_floor").to_owned(), id: 1, enabled: !state.doc.selection().is_empty() }]
    }

    fn context_command(&mut self, state: &mut AppState, id: u32) {
        if id == 1 {
            state.request(Action::MoveToFloor);
        }
    }
}
```

Tests: `crates/hp-ui-viewports/src/tests_hooks.rs` (wheel, context menu,
middle click).

## Picking

An eyedropper in a panel asks the user to click something in the views:

```rust
use hp_app::state::{PickKind, PickRequest, PickResult};

// Button clicked: start the request.
state.pick = Some(PickRequest { kind: PickKind::Entity, requester: "entity:target".to_owned(), result: None });

// Every frame: read and clear your result.
if let Some(req) = &state.pick
    && req.requester == "entity:target"
    && let Some(result) = req.result.clone()
{
    state.pick = None;
    if let PickResult::Entity(id) = result {
        // write the targetname of `id` into the key
    }
}
```

While `state.pick` has no result, `crates/hp-ui-viewports/src/pick.rs`
takes the left clicks of every view instead of the active tool (camera
navigation, the wheel and the context menu still work), shows a hint at the
top of each view and a crosshair cursor:

- `PickKind::Entity`: 3D, the first object hit (a brush entity solid gives
  its entity; a world brush gives nothing and the request stays pending);
  2D, the first entity under the point. Result `PickResult::Entity(id)`.
- `PickKind::Faces`: 3D only. Ctrl+click adds or removes a face
  (highlighted), a plain click adds the face and finishes, Enter finishes
  with the faces added so far. Result `PickResult::Faces(vec![(solid id,
  side index)])`; side ids for sidelist keys are
  `doc.scene().solid(id).solid.sides[index].id`.
- `PickKind::Point` and `PickKind::Angles`: 3D, the surface point; 2D, the
  grid snapped point at the depth of the selection center. Result
  `PickResult::Point(p)`; angles requesters compute the direction from
  their entity origin to `p`.
- Escape (no text field focused, no menu open) gives
  `PickResult::Cancelled`.

The `requester` string tells requests apart (one request at a time; a new
request replaces the old one and drops collected faces). Asset picks
(models, sounds, particles) are served by the browsers, not by the views.
Helpers usable elsewhere: `hp_ui_viewports::pick::{entity_at, face_at,
point_at}`, and `Viewports::pick_hint(state)` gives the translated hint
for the status bar.

Tests: `crates/hp-ui-viewports/src/tests_hooks.rs`.

## Grids and snapping

The grid a tool snaps to is not always Hammer's single size: the grid mode
(`[grid]` settings) can give each view its own grid, choose a custom grid
(a step per axis, an origin, an orientation, limits) of the map or of the
settings, or follow the grid attached to the selected objects. Snap with a
`hp_app::grid::GridFrame`, never with `state.grid_size()` alone:

- `state.grid_frame(view.kind)`: the active grid of a view (drawing new
  things);
- `state.edit_grid_frame(view.kind)`: the grid to edit the selection with
  (its own grid when it carries one and `grid.object_grids` is on);
- `state.surface_grid_frame(view.kind, Some(obj))`: drawing on an object;
- `frame.snap_2d(kind, p)` (points of a 2D view), `frame.snap_point(p)`,
  `frame.on_plane(p, n)`, `frame.snap_delta(reference, raw)` (moves),
  `frame.axis(k).snap(v)` (one world coordinate, box edits).

`state.grid_size()` is the size the grid controls show (the smallest step
of the grid they act on), fine for offsets like Duplicate. Custom grids of
the map live in the `hammerpro_grids` block (`hp_editor::GRIDS_BLOCK`), the
grid of an object in the `hammerpro_grid` key of its `editor` block; change
them through `AppState::put_custom_grid`, `delete_custom_grid` and
`Document::set_object_grid` (one undo step each).

## Editor extension

Editing operations are `impl Document` blocks in the package's own module of
`crates/hp-editor/src/`. These modules are already declared (empty, with a
header naming the owner), so no package edits `lib.rs` or `ops/mod.rs`:

| Module | Owner |
|---|---|
| `ops/path_nodes.rs`, `ops/origins.rs` | selection (1) |
| `describe.rs` | workflow (3) |
| `instances.rs`, `ops/instance.rs` | instances (4) |
| `ops/visgroup.rs` | visibility (5) |
| `side_index.rs` | helpers (6) |
| `ops/find_replace.rs` | entities (8) |
| `luxels.rs` | lighting (10) |
| `prefab.rs` | placement (11) |
| `compile_export.rs`, `fingerprint.rs` | compile (12) |
| `deps_roots.rs` | content (15) |
| `limits.rs`, `edicts.rs` | health (16) |
| `diff.rs`, `gallery.rs` | lab (17) |

Existing files keep their owners from the backlog (`ops/transform.rs`
selection, `ops/csg.rs` and `ops/create.rs` brush, `ops/disp.rs` mesh,
`ops/entity.rs` entities, `visibility.rs` visibility, `problems.rs` and
`stats.rs` health). Top-level modules are `pub`, so types you define there
are reachable as `hp_editor::fingerprint::Fingerprint` without editing
`lib.rs`; `ops/` modules are private (add methods to `Document`).

An undoable operation validates first, then mutates inside `self.tx(label,
|doc| ...)` (one undo step, or part of the transaction the caller opened)
through the recording writers of `document.rs` (`write_solids`,
`update_solids`, `update_entity`, `add_entity`, `remove_entities`,
`world_mut`, `misc_mut`, `visgroups_mut`, `extra_blocks_mut`, `sel_mut`).
They record the before state and the `Changes` the renderer syncs. Undo
labels are plain English; the shell translates them through
`shell.undo.<snake_case>` keys. Add the key for a new label to your
package's table in `crates/hp-app/src/actions/<package>.rs` (see
"Translations" below), not to hp-ui-shell, or reuse an existing label.

```rust
// crates/hp-editor/src/ops/origins.rs
use crate::{Document, EditResult, Id};

impl Document {
    /// Set `origin` of brush entities to the center of their solids. One undo step.
    pub fn center_origins(&mut self, entities: &[Id]) -> EditResult<usize> {
        // Validate and compute first (`origin_center` is a helper of this file).
        let moves: Vec<(Id, String)> = entities.iter().filter_map(|id| self.origin_center(*id)).collect();
        if moves.is_empty() {
            return Ok(0);
        }
        self.tx("Center origins", |doc| {
            for (id, origin) in &moves {
                doc.update_entity(*id, |e| e.set("origin", origin.as_str()));
            }
            Ok(moves.len())
        })
    }
}
```

Put unit tests in your module (`#[cfg(test)] mod tests`, like
`extra.rs`) rather than in `src/tests/`, whose `mod.rs` is shared.

### Extra blocks (Hammer++ `*_plus`)

Top-level blocks Hammer Pro keeps without interpreting them (`light_plus`,
`postprocess_plus`, `colorcorrection_plus`, `palette_plus`, `bgimages_plus`,
unknown blocks) are read and written with (`crates/hp-editor/src/extra.rs`):

- `doc.extra_block(name) -> Option<&hp_kv::Block>` (case insensitive)
- `doc.set_extra_block(name, block) -> EditResult<()>`: replaces the block in
  place (or adds it at the end), one undo step labelled "Edit map
  properties" (or part of your open transaction, so `doc.begin("Edit
  palette")` gives your own label), marks the map dirty, sets
  `Changes::extra_blocks_changed`. Reserved names (`world`, `entity`,
  `cameras`, `cordons`...) are refused.
- `doc.remove_extra_block(name) -> EditResult<Option<Block>>`

```rust
let mut block = doc.extra_block("light_plus").cloned().unwrap_or_else(|| hp_kv::Block::new("light_plus"));
block.set("samples_sun", "12");
doc.set_extra_block("light_plus", block)?;
```

Maps written by Hammer++ round-trip byte for byte (test
`extra::tests::hammer_plus_blocks_round_trip_and_edit`).

## Compile and launch

### Launch arguments (`crates/hp-compile/src/launch.rs`, launch (14))

The game command line moved from `hp-ui-mapping/src/compile/plan.rs` into
the public module `hp_compile::launch` (also re-exported at the crate root):

- `DEFAULT_GAME_ARGS`, `DEFAULT_STEAM_URL`, `LaunchMethod { Exe, SteamUrl }`
- `has_flag(args, flag) -> bool` (case insensitive)
- `game_args_for(user_args, game_folder) -> String`: adds `-steam -game
  <folder>`, `-hijack` and `+map {map}` unless the user set them; the result
  goes to `PlanOptions::game_args`.
- `launch_step(tools, map_name, opts) -> Step`: the `LaunchGame` step (game
  executable with `{map}` replaced, or the Steam URL through the system
  handler), used by `CompilePlan::preset`.
- `url_encode`, `url_opener`

Launch options (windowed, novid, running game reuse) are built here. Their
UI goes in `crates/hp-ui-mapping/src/compile/launch.rs`: `options_ui(ui,
state, enabled)` is already called by the compile dialog under its settings
(it draws nothing yet), so the launch package never edits `compile/mod.rs`.

### Map dialogs and module stubs (`crates/hp-ui-mapping`)

New modules are declared empty, each with its translation table:

| Module | Owner | Table (`src/tr/`) and key prefix |
|---|---|---|
| `cordons.rs` | visibility (5) | `cordons.rs`, `cordons.` |
| `portals.rs`, `areaportals.rs` | health (16) | `portals.rs` / `areaportals.rs` |
| `release/mod.rs`, `inspector.rs` | content (15) | `release.rs` / `inspector.rs` |
| `lighting.rs` | lighting (10) | `lighting.rs`, `lighting.` |
| `prefabs.rs` | placement (11) | `prefabs.rs`, `prefabs.` |
| `propper.rs` | lab (17) | `propper.rs`, `propper.` |
| `dialogs/instance.rs` | instances (4) | `instancing.rs`, `instancing.` |
| `compile/launch.rs` | launch (14) | `launch.rs`, `launch.` |

Floating windows implement the crate private trait `dialogs::MapDialog`
(`fn show(&mut self, ctx: &egui::Context, state: &mut AppState)`) and are
returned by the module's `dialogs()` function; `MapDialogs` (owned by the
binary) shows them every frame after the built-in dialogs. A window decides
itself when it is open (a `state.ui` flag, a requested action).

```rust
// crates/hp-ui-mapping/src/portals.rs
pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(PortalDialog::default())]
}

#[derive(Default)]
struct PortalDialog {
    open: bool,
}

impl MapDialog for PortalDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !self.open {
            return;
        }
        egui::Window::new(t!("portals.title")).open(&mut self.open).show(ctx, |ui| {
            // ...
        });
    }
}
```

Dock tabs (lighting, prefabs) are hosted by the binary like the other
panels; their modules are public (`hp_ui_mapping::lighting::LightingTab`).

### Translations of hp-ui-mapping

The table is split per module in `crates/hp-ui-mapping/src/tr/` (`compile.rs`
compile, `problems.rs` and `mapinfo.rs` health, `transform.rs` and
`paste.rs` selection, `goto.rs` and `pointfile.rs` compile, `visgroups.rs`
and `autovis.rs` visibility, `outliner.rs`, plus the new modules above).
`tr/mod.rs` lists `TABLES: &[(prefix, table)]`, registered by
`register_translations()`. Every key of a table starts with its prefix and
a dot (tested), keys are unique across tables, and English and French
texts are required. Before choosing a new prefix, search the workspace for
it (`rg '"myprefix\.'`): tables of different crates share one registry.

## Actions

Every command of the application (menus, toolbar, command palette, key
bindings, F1 reference, Settings > Keyboard) is an `hp_app::Action`. Rust
needs one enum, but its entries are written in one file per package and
joined by a chain of macro callbacks into the single `collect_actions!`
invocation of `actions/mod.rs`.

Files, in `crates/hp-app/src/actions/`:

| File | Content / owner |
|---|---|
| `mod.rs` | the macros, `Action::{ALL, DECLARED}`, `Category`, `ActionStatus`, `Check`, the per-package lookups (foundation) |
| `groups.rs` | `MenuGroup` (menu placement) and the titles of new submenus (shared, see [Menus](#menus)) |
| `keys.rs` | `KeyBindings`, shortcut parsing, formatting and localized labels (foundation) |
| `shell.rs` | the 91 core actions; their ids, shortcuts and order never change (tested) |
| `selection.rs` ... `lab.rs` | one file per package (all 17 exist) |

Every action the backlog plans is already declared in its package file, with
the status `Planned`:

| Package | Pre-declared actions (default shortcut) |
|---|---|
| selection (1) | MoveToFloor (Alt+N), MoveToCeiling (Alt+U), SnapToGridIndividually (Ctrl+Shift+B), CenterOrigins, TogglePivot, FlipHorizontal (Ctrl+L), FlipVertical, AlignCenterHorizontal, AlignCenterVertical, ToggleGizmoLocal (Ctrl+J) |
| brush (2) | ToolPolygon (Shift+N), MergeBrushes (Ctrl+Shift+M), MakeHintBrush, NodrawHiddenFaces, TieEachToEntity (Ctrl+Shift+T), ConvertToDetail |
| workflow (2) | CloseMap (Ctrl+F4), ToggleIgnoreGroups (Ctrl+W), Zoom2dIn (+), Zoom2dOut (-), ResetViewSplits, ViewTop (F2), ViewSide (F3), ViewFront (F4), View3dTextured (F5), View3dFlat (Shift+F5), CycleView2d (Tab), ToggleViewInfo (O), ToggleBoundsOverlay, ShowHistory, ShowToolbar, ShowToolPalette, ShowStatusBar, ToolPaletteTwoColumns, ToolbarTwoRows, ResetToolbar, SaveLayout, ResetLayout |
| instances (4) | CreateInstance (Alt+S), EditInstance, CollapseInstances, CollapseAllInstances, InstancesHidden, InstancesTinted, InstancesNormal |
| visibility (5) | ToolCordon, ToggleCordon, MoveToVisgroup, HiddenToVisgroup |
| helpers (6) | ToggleOverlays, ToggleConnections, ToggleEditorObjects, TogglePointEntities, ToggleEntityNames, SimulateRopes |
| mesh (7) | VertexSplitFace (Ctrl+F), VertexScale (Alt+E), DispSubdivide, DispInvertAlpha, DispNoise, DispToMesh, MeshToDisp, DispConformMesh, DispTileSelection |
| entities (8) | CopyKeyvalues, PasteKeyvalues, FindReplaceText (Ctrl+Alt+R), EditColor (C), EditSoundscapes, ShowSoundBrowser, ShowParticleBrowser, EntityHelp, ToolPath, PathInsertPoint, PathRemovePoint |
| render (9) | ToggleGrid3d (P), FarPlaneCloser, FarPlaneFarther, RenderUnshaded (Alt+6), RenderSmoothingGroups (Alt+7), RenderColorCorrected (Alt+9), ToggleDetailPreview, ToggleRenderEffects, ToggleSkyPreview, ToggleSkybox3d (Alt+V), ToggleCollisionModels, ToggleFogPreview, ToggleAnimatedMaterials, ToggleModelFade, ToggleOpaqueMaterials, ToggleWalkable, ToggleDisplacements, BackgroundImages |
| lighting (10) | RenderLightmapGrid (Alt+8), RenderBaked (Ctrl+F5), BakeLighting, BakeAroundCamera, StopBake, SelectTinyLightmapFaces, ShowLighting |
| placement (11) | ToolSprinkle, CreatePrefab (Ctrl+R), InsertOriginalPrefab (Alt+Shift+C), ShowPrefabs |
| compile (12) | CompileNow, CancelCompile, BatchCompile |
| startup (13) | ExportCopy, ExportAgain (Alt+B), ExportObj, ReloadContent |
| launch (14) | LaunchGame, ReloadInGame, ImportFromGame |
| content (15) | SaveAsNewVersion, OpenArchive, BuildContent, PublishWorkshop |
| health (16) | CheckLeaks, SealAreaportal, LoadPortalFile, UnloadPortalFile, TogglePvs |
| lab (17) | OpenBsp, CompareMaps, EntityGallery, ShowLogicalView, ToggleNavMesh, PhysicsPlacement, BrushesToProp, GenerateNavMesh |

An entry is one line:

```rust
// Variant => "id", Category, MenuGroup, default shortcut, status;
MoveToFloor => "move_to_floor", Tools, ToolsBrush, sc(ALT, Key::N), Ready;
```

- `"id"`: stable, stored in settings files (key bindings) and used for the
  label key `action.<id>`. Never change it once it is `Ready`.
- `Category`: the top level menu (`File`, `Edit`, `Map`, `View`, `Tools`,
  `Help`), also the grouping of the palette and of the keyboard page.
- `MenuGroup`: where the entry shows in that menu (see [Menus](#menus));
  `Hidden` keeps it out of the menus (palette and shortcut only).
- shortcut: `None` or `sc(MODS, Key::X)` with `NONE`, `CTRL`, `SHIFT`, `ALT`,
  `CTRL_SHIFT`, `CTRL_ALT`, `ALT_SHIFT`. Defaults are unique across every
  package, planned actions included (test `shortcuts_unique`).
- status: `Planned` actions are declared but invisible (not in
  `Action::ALL`: no menu entry, no palette entry, no keyboard page row, and
  their shortcut does nothing, so pre-declaring changed nothing for users).
  Switch it to `Ready` when your handler works: the action then appears in
  its menu group, the palette, F1 and Settings > Keyboard, and its shortcut
  is live. `Action::DECLARED` lists everything, `Action::from_id` finds
  planned ids too.

Next to the list, each package file has (already written, returning nothing):

- `pub(super) const REPEATS: &[Action]`: actions that repeat while their key
  is held (zoom steps); the others fire once per press.
- `pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool>`:
  greyed out in menus and toolbars when `Some(false)`. `None` means "not
  mine / no condition" (enabled). Planned actions are never enabled.
- `pub(super) fn check(state: &AppState, action: Action) -> Option<Check>`:
  `Check::Toggle(on)` or `Check::Radio(on)` for menu check marks.
- `pub(super) fn icon(action: Action) -> Option<&'static str>`: a glyph of
  egui's bundled fonts (palette and toolbar icons). Check it exists: the
  `icon_glyphs_exist` test of hp-ui-shell covers every action.
- `pub(super) const TR: TrTable`: the labels `action.<id>` (and the other
  texts of your actions, see [Translations](#translations)).

Example, shipping "Move to floor" (`crates/hp-app/src/actions/selection.rs`):

```rust
action_list! { selection_actions {
    MoveToFloor => "move_to_floor", Tools, ToolsBrush, sc(ALT, Key::N), Ready;
    MoveToCeiling => "move_to_ceiling", Tools, ToolsBrush, sc(ALT, Key::U), Ready;
    // ...
}}

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::MoveToFloor | Action::MoveToCeiling => Some(!state.doc.selection().objects.is_empty()),
        _ => None,
    }
}
```

then handle it in `crates/hammerpro/src/dispatch/selection.rs` (see
[Dispatch](#dispatch)). An action missing from the backlog: add its line
below the `// Package N: add new actions below this line.` marker of your
file and its label to your `TR`; nothing else changes.

Tool switches: `ToolId::{Path, Polygon, Cordon, Sprinkle}` are declared in
`crates/hp-app/src/tool.rs` with their actions `ToolPath`, `ToolPolygon`, `ToolCordon`,
`ToolSprinkle`, labels `tool.path`, `tool.polygon`, `tool.cordon`, `tool.sprinkle` and
icons. The tool palette shows a tool once its action is `Ready`; until a
real tool with that id is registered (your crate's `tools()` or
`Package::tools`), the binary keeps a placeholder and the action shows "not
implemented yet". A tool can also take any requested action while it is
active with `Tool::action(&mut self, state, action) -> bool` (for example
the vertex tool taking `SnapToGrid` or `VertexSplitFace`).

The tool palette and a few toolbar buttons (Carve, the hide and group
buttons, Compile) do not use the glyph: they draw a vector icon from
`crates/hp-ui-shell/src/icons.rs` (`icons::paint`, one arm per `ToolId` in
`paint_tool`, sized on an 18 point grid with strokes snapped to pixels so it
stays sharp at 100, 125 and 150 percent). A new `ToolId` needs its arm there
(the match is exhaustive); the glyph of `ToolId::icon` stays for the menus.
The toolbar layout is the `TOOLBAR` table of `crates/hp-ui-shell/src/toolbar.rs`
(groups with an id, a label and their items); the user's order and hidden
groups or buttons live in `workflow.toolbar_order` and `workflow.toolbar_hidden`
by id, so a new button only needs its entry in a group.

Known overlap: F9 belongs to `RunMap` ("Compile and run..."). When
`CompileNow` ships with F9 (backlog: F9 compiles at once, Ctrl+F9 opens the
dialog), package 12 changes the two shortcuts together, the one exception
where a package edits `shell.rs`; the core test then needs the same update.

Tests: `cargo test -p hp-app` (ids unique and translated, shortcuts unique,
core ids, shortcuts and order unchanged, one menu per group, planned
shortcuts inactive), `cargo test -p hp-ui-shell` (menu layout, glyphs),
`cargo test -p hammerpro` (every ready action has exactly one handler).

## Settings

`hp_app::settings::Settings` nests one serde struct per package, each in its
own file and saved as its own TOML table in `settings.toml`
(`%APPDATA%\HammerPro\settings.toml`).

Files, in `crates/hp-app/src/settings/`:

| File | Content / owner |
|---|---|
| `mod.rs` | `Settings`, paths (`config_dir`, `cache_dir`, `autosave_dir`...), load and atomic save (foundation) |
| `shell.rs` | `[view]` `ViewSettings`, `[edit]` `EditSettings`, `[ui]` `UiSettings` and their enums: core, shared, do not add fields |
| `startup.rs` | `[game]` `GameSettings` and `[startup]` `StartupSettings`: startup (13) |
| `compile.rs` | `[compile]` `CompileSettings`: compile (12) |
| `entities.rs` | `[browser]` `BrowserSettings` and `[entities]` `EntitiesSettings`: entities (8) |
| `<package>.rs` | `[selection]` `SelectionSettings`, `[brush]` `BrushSettings`, `[workflow]` `WorkflowSettings`, `[instances]`, `[visibility]`, `[helpers]`, `[mesh]`, `[render]`, `[lighting]`, `[placement]`, `[launch]`, `[content]`, `[health]`, `[lab]` |

The backlog often says "add `EditSettings.x`" or "`ViewSettings.x`": put
`x` in your own struct instead and read it as `state.settings.<package>.x`.

Rules:

- The struct has `#[serde(default)]`: every field needs a default, and a
  file written before the field existed loads with that default. When a
  field needs a non-zero default, replace `#[derive(Default)]` by an
  `impl Default`.
- Never rename or retype a stored field; keep the old name with
  `#[serde(alias = "old_name")]` (also for enum variants).
- Package tables are written only when they differ from their defaults
  (`skip_serializing_if`), so empty packages leave no trace in the file, and
  unknown tables (a newer version) are ignored.
- Edit `state.settings` directly and set `state.settings_dirty = true`; the
  binary saves the file (debounced) and on exit.

```rust
// crates/hp-app/src/settings/selection.rs
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectionSettings {
    /// Rotation handle step in degrees.
    pub rotation_snap_deg: f64,
    pub clone_keeps_visgroups: bool,
}

impl Default for SelectionSettings {
    fn default() -> Self {
        Self { rotation_snap_deg: 15.0, clone_keeps_visgroups: true }
    }
}

// Anywhere: let step = state.settings.selection.rotation_snap_deg;
```

### Runtime state (not saved)

State that several crates of one package share during a session (the open
flag of a window requested by an action, a compile progress shown by a
status segment, a problem count) goes in
`crates/hp-app/src/runtime/<package>.rs` (`SelectionRuntime`,
`CompileRuntime`...), reachable as `state.runtime.<package>`, instead of new
fields in `AppState` or `UiState` (`state.rs` is shared). The structs only
need `Default` and `Debug`; nothing in them is saved. Per-map data goes in
the [Sidecar](#sidecar).

```rust
// crates/hp-app/src/runtime/compile.rs
#[derive(Debug, Default)]
pub struct CompileRuntime {
    /// Step and fraction done, for the status bar (None when idle).
    pub progress: Option<(String, f32)>,
}
```

## Dispatch

The binary runs the requested actions (`state.request(action)` from any
crate) at the end of the frame. One module per package in
`crates/hammerpro/src/dispatch/` holds that package's handlers and the other
application hooks.

| File | Content / owner |
|---|---|
| `mod.rs` | trait `Package`, the `PACKAGES` list, `HammerProApp::execute`, the "not implemented yet" notice, helpers, `Packages` (state of every package) (foundation) |
| `shell.rs` | core: file, edit, grid, view toggles, panels, groups, go to, map properties, help |
| `selection.rs` | paste special, snap to grid, transform, flip, align (moved) |
| `brush.rs` | tie to entity, move to world, carve, make hollow, merge (moved) |
| `visibility.rs` | hide selected, hide unselected, unhide all (moved) |
| `helpers.rs` | show helpers (moved) |
| `entities.rs` | find entities, replace textures, entity report (moved) |
| `render.rs` | 3D render modes, show models, show tool textures (moved) |
| `compile.rs` | compile and run dialog, leak pointfiles and their file picker (moved) |
| `health.rs` | check for problems, map information (moved) |
| `workflow.rs`, `instances.rs`, `mesh.rs`, `lighting.rs`, `placement.rs`, `startup.rs`, `launch.rs`, `content.rs`, `lab.rs` | empty, ready for their package |

`execute(action)` runs, in order: a tool switch (when the tool exists), the
active tool's `Tool::action`, then each package's `handle` in the order of
`PACKAGES` until one returns true. An action nobody handles (a planned
action requested by a script, an action switched to `Ready` before its
handler) shows a translated "<label> is not implemented yet" toast and logs
it once, so a menu entry never silently does nothing.

Each module is a unit struct implementing `dispatch::Package`; every method
has a do-nothing default:

| Hook | When |
|---|---|
| `handled(&self) -> &'static [Action]` | the actions `handle` runs; must match it exactly (debug assertion in `execute`, test `every_ready_action_has_one_handler`) |
| `handle(&self, app, ctx, action) -> bool` | run the action; true when handled |
| `tick(&self, app, ctx)` | every frame, before the UI pass (poll workers, file dialogs, timers) |
| `init(&self, app)` | once at startup (register settings pages, status segments, menu extensions, overlay layers with `app.viewports`) |
| `map_loaded(&self, app)` | after a map was opened or created, its sidecar loaded |
| `dialogs(&self, app, ctx)` | every frame, after the built-in windows |
| `dock_tabs(&self, app, tabs)` / `dock_ui(&self, app, ui, id) -> bool` | right dock tabs, shown as `RightTab::Package(id)` after the built-in tabs |
| `tools(&self, out)` | editing tools of the package, added when the tool list is built |
| `translations(&self) -> TrTable` | the module's table, registered at startup |

State the package keeps in the application goes in the module's `State`
struct (`#[derive(Default)]`), reachable as `app.pkg.<package>` (UI
components such as a dock panel, receivers of worker threads). Helpers for
handlers: `app.selected()`, `app.selected_solids()`,
`app.require_selection()`, `app.report(action, result)` (warning toast on
`Err`), `app.entity_bounds_fn()`, `dispatch::edit_error_text`.

```rust
// crates/hammerpro/src/dispatch/placement.rs (illustration: `PrefabTab`,
// `prefabs.tab` and `runtime.placement.create_open` come with package 11)
use hp_app::{Action, t};
use hp_app::state::RightTab;

use super::{DockTab, Package};
use crate::app::HammerProApp;

pub(crate) struct Placement;

#[derive(Default)]
pub(crate) struct State {
    prefabs: hp_ui_mapping::prefabs::PrefabTab,
}

const HANDLED: &[Action] = &[Action::ShowPrefabs, Action::CreatePrefab];

impl Package for Placement {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        match action {
            Action::ShowPrefabs => app.state.ui.right_tab = RightTab::Package("prefabs"),
            Action::CreatePrefab => app.state.runtime.placement.create_open = true,
            _ => return false,
        }
        true
    }

    fn dock_tabs(&self, _app: &HammerProApp, tabs: &mut Vec<DockTab>) {
        tabs.push(DockTab { id: "prefabs", label: t!("prefabs.tab").to_owned() });
    }

    fn dock_ui(&self, app: &mut HammerProApp, ui: &mut egui::Ui, id: &'static str) -> bool {
        if id != "prefabs" {
            return false;
        }
        app.pkg.placement.prefabs.ui(ui, &mut app.state);
        true
    }
}
```

## Menus

The menu bar (`crates/hp-ui-shell/src/menu.rs`) is built from the action
registry every frame: a `Ready` action appears in the menu of its `Category`
at the place of its `MenuGroup`, with its label, current shortcut, check
mark and enabled state (from the package file, see [Actions](#actions)). No
package edits `menu.rs`.

A group (`crates/hp-app/src/actions/groups.rs`, `MenuGroup::info`) has a
section, an order and either no submenu (entries inline) or a submenu title
and width. A menu shows its groups sorted by (section, order) with a
separator between sections. Groups of one menu that share a submenu title
are merged into that one submenu, sorted by (section, order), with a
separator between them: this is how a submenu gets its own groups (the
submenu takes the place of its first group). Inside a group, entries follow
`hammer_order` in `menu.rs` when the group has a list there, then the
registry order (core first, then the package files in `PACKAGES` order,
then line order).

| Menu | Entries (submenus in bold) |
|---|---|
| File | `FileNew` 10/0, recent maps (shell) 10/5, `FileClose` 10/20, `FileSave` 20, **Import** (`FileImport` 30/0, `FileImportOpen` 30/1), **Export** (`FileExport` 30/10), **Compile** (`FileRun` 40/0, `FileGame` 40/1), **Publish** (`FilePublish` 40/10), `FileExit` 90 |
| Edit | `EditHistory` 10, `EditClipboard` 20, `EditSelect` 30/0, **Selection mode** (`EditSelectMode` 30/10, `EditSelectGroups` 30/11), **Selection element** (`EditSelectElement` 30/20, `EditSelectEdges` 30/21, `EditFaceHistory` 30/22), `EditFind` 40, `EditProperties` 50/0, **Display colors** (`EditColors` 50/10) |
| Map | `MapGrid` 10/0, **Grids** (`MapGrids` 10/10), `MapInfo` 20/0, **Leaks and visibility** (`MapPointfile`, `MapPortals`, `MapOptimize`, `MapNavMesh` 20/10 to 20/13), `MapProperties` 30, **Lighting** (`MapLighting`, `MapLights`, `MapCubemaps` 40/0 to 40/2) |
| View | **View type** (`ViewType` 10/0), **3D render mode** (`ViewRender` 10/10), `ViewPlayer` 10/20, **Display options** (`ViewShow`, `ViewDisplay`, `ViewDisplay2d`, `ViewDisplayInfo` 20/0 to 20/3), **Environment preview** (`ViewSky`, `ViewFog`, `ViewWater`, `ViewEnvironment` 20/10 to 20/13), **Effects** (`ViewEffects`, `ViewEffectsObjects` 20/20, 20/21), **Diagnostics** (`ViewDiagnostics` 20/30), `ViewHide` 30, `ViewNav` 40/0, **Zoom and draw distance** (`ViewZoom` 40/10), **Panels** (`ViewPanels` 50/0), language (shell) 50/10 |
| Tools | **Active tool** (`ToolsSwitch` 10), `ToolsBrush` 20/0, **Arrange** (`ToolsAlign`, `ToolsFlip`, `ToolsArrange`, `ToolsOrigin` 20/10 to 20/13), `ToolsGroup` 30, `ToolsCsg` 40/0, **Brush operations** (`ToolsBrushOps`, `ToolsFaceOps`, `ToolsBrushMisc` 40/10 to 40/12), **Vertices and edges** (`ToolsMesh` 40/20), **Displacements** (`ToolsDisp`, `ToolsDispSlope`, `ToolsDispNoise`, `ToolsTerrain` 40/30 to 40/33), **Curves and roads** (`ToolsCurves`, `ToolsRoads` 40/40, 40/41), **Textures** (`ToolsTextures` 40/50), **Placement and props** (`ToolsPlaceObjects`, `ToolsPlacement`, `ToolsScatter`, `ToolsProps`, `ToolsPaths` 60/0 to 60/4; `ToolsPlaceObjects` is titled **Place on real shapes** in the viewport context menu), **Instancing** (`ToolsInstancing`, `ToolsInstanceDisplay` 60/10, 60/11), **Content and editors** (`ToolsContent`, `ToolsContentIndex` 60/20, 60/21), **Blender and MCP** (`ToolsBlender`, `ToolsMcp` 60/30, 60/31), `ToolsApp` 90 |
| Window | **Layout** (`WindowLayout` 10/0, saved layouts extension 10/1), `WindowMessages` 20, `WindowMaps` 30 |
| Help | `HelpDocs` 10, `HelpSupport` 20/0, setup assistant (shell) 20/10, `HelpAbout` 90 |

The menus follow Valve Hammer (File, Edit, Map, View, Tools, Window, Help)
so a mapper coming from Hammer or Hammer++ finds the entries where they
expect them: Undo and Redo then Cut, Copy and Paste at the top of Edit, the
grid in Map, Carve, Group, Transform and Options in Tools, hiding in View,
the messages and layouts in Window. Frequent commands stay inline at the top
of their menu and the rest goes into submenus, so no top level menu is
taller than about twenty entries (`no_menu_is_taller_than_about_twenty_entries`
in `menu.rs` checks it, and `every_entry_of_the_2_0_2_menus_is_still_in_the_menus`
checks against `menu_ids_2_0_2.txt` that a reorganization never drops an
entry). A menu without any entry is not shown. The category of an action
must be the menu of its group (`MenuGroup::category`, checked by a test).
`MenuGroup::title` gives the name of a group on its own (Align and Flip
keep their names in the viewport context menu while they share **Arrange**
in the menu bar). `hp_ui_shell::menu::menu_path` returns the menu and
submenu titles of an action: the command palette and the MCP `list_actions`
tool (`menu_path`) show it.

Every group the backlog needs exists; adding one is an edit of the shared
`groups.rs` (a variant, its `info` line and the submenu title in its `TR`):
coordinate it.

Content that is not an action (a list of saved layouts, fog controllers,
recent instances) is a `hp_app::extend::MenuExtension`: a closure placed by
menu, section and order like a group. `.in_submenu("menu.x")` puts it inside
the submenu with that title instead: at the end of the group of that
submenu whose (section, order) is the same, or as its own group otherwise. Register it from your package's
`init` hook; draw entries with `hp_ui_shell::menu::menu_entry` (same check
column and shortcut layout as the actions) and `hp_ui_shell::menu::submenu`:

```rust
// crates/hammerpro/src/dispatch/workflow.rs (illustration: the layout
// fields and keys come with package 3)
fn init(&self, app: &mut HammerProApp) {
    use hp_app::actions::Category;
    use hp_app::extend::MenuExtension;
    use hp_ui_shell::common::Check;
    use hp_ui_shell::menu::{menu_entry, submenu};

    app.shell.add_menu_extension(MenuExtension::new(Category::Window, 10, 1, |ui, state| {
        submenu(ui, t!("workflow.saved_layouts"), |ui| {
            for name in state.settings.workflow.layouts.keys() {
                if menu_entry(ui, name, None, true, Check::None).clicked() {
                    state.runtime.workflow.apply_layout = Some(name.clone());
                }
            }
        });
    }).in_submenu("menu.layout"));
}
```

An extension must draw at least one entry (an empty one would leave two
separators side by side). The viewport context menu is a different thing,
owned by hp-ui-viewports (see Tool hooks).

## Settings pages

The settings window (`crates/hp-ui-shell/src/settings_dialog/`) has one file
per page:

| File | Page / owner |
|---|---|
| `mod.rs` | window, page list, footer ("Restore defaults", "Apply and reload content"), `SettingsTab` (foundation) |
| `general.rs` | General: language, theme, scale, autosave (workflow 3) |
| `game.rs` | Game (startup 13) |
| `viewport.rs` | Viewport: camera, rendering, colors (workflow 3) |
| `editing.rs` | Editing (core) |
| `compile.rs` | Compile (compile 12) |
| `keyboard.rs` | Keyboard (workflow 3) |
| `selection.rs`, `brush.rs`, `workflow.rs`, `instances.rs`, `visibility.rs`, `helpers.rs`, `mesh.rs`, `entities.rs`, `render.rs`, `lighting.rs`, `placement.rs`, `startup.rs`, `launch.rs`, `content.rs`, `health.rs`, `lab.rs` | `pages()` of each package (empty) |

A package page implements `hp_app::extend::SettingsPage` and is returned by
`pages()` in the package's file, or written in any crate and registered with
`app.shell.add_settings_page(Box::new(..))` from the package's `init` hook.
Pages are sorted by `order()` among the built-in ones
(`hp_app::extend::page_order`: General 100, Game 200, Viewport 300, Editing
400, Compile 500, Keyboard 600). "Restore defaults" calls `reset` (hide it
with `can_reset() -> false`). Open a page from code with
`shell.open_settings(state, SettingsTab::from_id("selection"))`, or from a
status segment with `cx.open_settings = Some("selection")`.

```rust
// crates/hp-ui-shell/src/settings_dialog/selection.rs
use hp_app::extend::{SettingsPage, page_order};
use hp_app::settings::SelectionSettings;
use hp_app::{AppState, t};

pub(super) fn pages() -> Vec<Box<dyn SettingsPage>> {
    vec![Box::new(SelectionPage)]
}

struct SelectionPage;

impl SettingsPage for SelectionPage {
    fn id(&self) -> &'static str {
        "selection"
    }
    fn label(&self) -> String {
        t!("shell.selection.page").to_owned()
    }
    fn icon(&self) -> &'static str {
        "⬉"
    }
    fn order(&self) -> i32 {
        page_order::EDITING + 10
    }
    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let s = &mut state.settings.selection;
        if ui.add(egui::DragValue::new(&mut s.rotation_snap_deg).range(1.0..=90.0).suffix("°")).changed() {
            state.settings_dirty = true;
        }
    }
    fn reset(&mut self, state: &mut AppState) {
        state.settings.selection = SelectionSettings::default();
    }
}

pub(crate) const TR: hp_app::i18n::TrTable = &[("shell.selection.page", "Selection", "Sélection")];
```

Each page file (and each status file) already has its `TR` table listed
in `settings_dialog::TABLES` (`status::TABLES`), registered with the shell:
fill it, nothing else to edit.

The Compile page also draws the options menus of the compile steps (every
flag of `hp_compile::flags::FLAGS`), which live in `hp-ui-mapping`: the
compile package registers them from its `init` hook with
`hp_app::extend::set_compile_options_ui(hp_ui_mapping::compile_options_ui)`.

## Status bar

The status bar (`crates/hp-ui-shell/src/status/`) is a list of
`hp_app::extend::StatusSegment`s:

| File | Segments / owner |
|---|---|
| `mod.rs` | `StatusBar`: layout and registration (foundation) |
| `shell.rs` | tool hint, selection summary, cursor position (left); fps, texture lock, snap, grid (right; the last three only while the toolbar, or the toolbar group or button showing them, is hidden); `fmt_size`, `fmt_point` (core, workflow 3 extends it) |
| `startup.rs` | game content indicator: mounting, not configured, failed (startup 13) |
| `compile.rs`, `health.rs`, `launch.rs`, `lighting.rs` | empty `segments()` for compile progress, problem and limit badges, game ready, lightmap budget |

Layout: right side segments are drawn from the right edge toward the middle,
sorted by `order()`; each draws its own separator before its content (when it
shows anything). Then the bar draws a separator and the left side segments
left to right, separated by the bar, each in its own clipped rectangle so they
never overlap. A left segment describes its room with `fit()`
(`hp_app::extend::StatusFit`: `fixed`, `shrinkable(min, preferred)` or
`flex(min)` and a priority from `status_priority`, or `None` to take no room
at all); the default `fit()` turns `width()` into a fixed width and `None` into
a flexible one. `status::fit_widths` gives every segment its minimum, grows the
fixed ones toward their preferred width, gives the rest to the flexible ones,
and when even the minimums do not fit leaves out the lowest priority segments
first (REAL_DIMS 10, HINT 20, SELECTION 30, CURSOR 40). Order
constants are in `hp_app::extend::status_order` (left: HINT 100, SELECTION
200, CURSOR 300; right: FPS 100, TEXTURE_LOCK 200, SNAP 300, GRID 400,
CONTENT 500). `StatusContext` gives the hint of the frame (`tool_hint`: the
compile running, a pending pick or the active tool) and takes requests
(`open_settings`). Data computed in another crate reaches a segment through
`state.runtime.<package>`. Segments written outside hp-ui-shell are added
with `app.shell.add_status_segment(..)` from `init`.

```rust
// crates/hp-ui-shell/src/status/compile.rs
use egui::Ui;
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::{Action, AppState};

pub(super) fn segments() -> Vec<Box<dyn StatusSegment>> {
    vec![Box::new(CompileProgress)]
}

struct CompileProgress;

impl StatusSegment for CompileProgress {
    fn id(&self) -> &'static str {
        "compile_progress"
    }
    fn side(&self) -> StatusSide {
        StatusSide::Right
    }
    fn order(&self) -> i32 {
        status_order::CONTENT + 10
    }
    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        // Illustration: `progress` is a field package 12 adds to its runtime struct.
        let Some((step, done)) = state.runtime.compile.progress.clone() else { return };
        ui.separator();
        // Right to left: the Cancel button ends up right of the bar.
        if ui.small_button("✖").clicked() {
            state.request(Action::CancelCompile);
        }
        ui.add(egui::ProgressBar::new(done).text(step).desired_width(160.0));
    }
}
```

## Sidecar

Per-map editor data that does not belong in the VMF (view state, ignored
problems, last compile fingerprint, Workshop id, game profile) is stored in
a sidecar file: `crates/hp-app/src/sidecar.rs`.

- One TOML file per map: `<cache dir>/maps/<map key>.toml`
  (`%LOCALAPPDATA%\HammerPro\maps\` on Windows). The key is
  `hp_app::sidecar::map_key(path)`, `<stem>_<hash of the full path>`, the
  same as the autosave names.
- One table per named section; a package reads and writes only its own
  sections. Names: your package key (`"compile"`) or key and topic
  (`"health_ignored"`); `"sidecar"` is reserved (format version and map
  path).
- `state.sidecar` is the sidecar of the open map. The binary
  (`crates/hammerpro/src/files.rs`) loads it when a map is opened or
  created, writes it atomically (temporary file then rename) when the map is
  saved (Save as moves the sections to the new name) and when it is closed
  (new map, open another, exit). Maps without a path keep it in memory
  until their first save; automated runs (`--script`, `--screenshot`) never
  write it.
- A missing, unreadable or corrupt file, or a section that does not match
  its type any more, reads as defaults. Never keep the only copy of user
  work there.

API: a section type implements `SidecarSection` (`Serialize +
DeserializeOwned + Default` and `const NAME`); then `state.sidecar.get::<T>()`
and `state.sidecar.set(&value)` (an unchanged value does not mark the
sidecar dirty). Untyped: `get_named`, `set_named`, `remove`, `contains`.
`state.sidecar.serial()` changes with every map loaded. Read your sections
in the `map_loaded` hook of your dispatch module (or compare `serial()` when
you cache them elsewhere) and write them whenever the value changes:
writing in memory is cheap, the file is written on save and close.

```rust
// crates/hp-editor or any crate of the package: the section type
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct IgnoredProblems {
    pub keys: Vec<String>,
}

impl hp_app::sidecar::SidecarSection for IgnoredProblems {
    const NAME: &'static str = "health_ignored";
}

// crates/hammerpro/src/dispatch/health.rs
fn map_loaded(&self, app: &mut HammerProApp) {
    let ignored: IgnoredProblems = app.state.sidecar.get();
    app.state.runtime.health.ignored = ignored.keys;
}

// Where the user clicks "Ignore":
state.sidecar.set(&IgnoredProblems { keys: state.runtime.health.ignored.clone() });
```

Tests: `hp_app::sidecar::tests` (round trip, corrupt files, save as) and
`files::tests::the_sidecar_follows_the_map` (open, save, close).

## Several open maps (tabs)

Several maps can be open at once (`crates/hammerpro/src/tabs.rs`). Every
crate keeps working on `state.doc`, the map shown; the binary parks the
others with their cameras, sidecar, pointfile and pivot, and swaps them
when the user switches tabs (Ctrl+Tab, a click in the tab bar above the
views). What follows for a package:

- A switch is a map change: `doc.mark_everything_changed()` runs,
  `state.pick` is dropped, and the sidecar of the map shown becomes
  `state.sidecar`, so `Package::map_loaded` runs as after an open. Keep
  per-map state in the sidecar (or recompute it in `map_loaded`), never in
  a package `State` that outlives the map.
- The renderer scene of the map left is parked (`AssetsApi::park_scene`,
  `hp_render::ParkedScene`): showing that map again restores its objects,
  chunks, model instances, sprites, lights and the scene producers with
  their state, without a rebuild (producers do not get `full` then). A
  map opened for the first time, or changed while hidden, is rebuilt (big
  maps upload over several short frames).
- `Document::revision` is unique in the process: two maps never share a
  revision, so a cache keyed by it never mixes them. Object ids are not:
  key nothing by an id alone.
- Opening a map (`state.requested_open`, recent maps, a dropped file, an
  instance to edit) gives it a new tab, or shows its tab when it is open
  already. `state.runtime.workflow.tabs` lists the open maps (id, title,
  path, unsaved changes) and `state.runtime.workflow.tab_request` asks
  for a switch or a close (`TabRequest`).
- Close (Ctrl+F4), Close other maps and quitting ask to save every
  modified map in turn; autosaves and emergency saves cover every open map.

## Look and colors

The UI is black and white only (`crates/hp-app/src/theme.rs`): a dark theme
of true black layers and its light mirror. States and severities read from
gray levels, weight, icons and inverted fills, never from hue. Color stays
in content only: the 3D and 2D views, thumbnails and previews, entity
colors, visgroup colors and color pickers.

| Need | Use |
|---|---|
| highlight, active item, focus, mark | `theme::accent()` |
| background of a selected item | `theme::accent_bg()` |
| text on an `accent()` fill (inverted buttons, pills) | `theme::on_accent()` |
| primary / secondary text, hairlines | `theme::text()`, `theme::text_weak()`, `theme::border()` |
| severities | `theme::info()`, `success()`, `warning()`, `error()` with the icon (`Severity::icon`) |
| every gray level of the theme | `theme::palette()` (`bg0` deepest to `bg4` hovered, `selection`, `strong`, `inverse`) |
| main action of a row or dialog | `widgets::primary_button` |
| on/off switch, state badge | `widgets::toggle`, `widgets::pill` |
| the Hammer Pro mark | `widgets::logo`, `widgets::paint_hammer` |
| tools drawing inside the views | `theme::VIEW_ACCENT`, `VIEW_SUCCESS`, `VIEW_WARNING`, `VIEW_ERROR`, `AXIS_*` |

The token functions follow the applied theme; the `ACCENT` and `COLOR_*`
constants are the dark theme values, only for surfaces that stay dark in
both themes (the overlays of the views). The test
`ui_crates_hold_no_hued_chrome_colors` (hp-app) scans the UI crates: a hued
color literal outside the content modules it lists, a `VIEW_*` or `AXIS_*`
color outside the view crates, or a dark only constant in a panel fails it.

Colors that carry meaning in the data (the node categories and pin types of
the shader editor, the R, G and B channels of the color grading curves, the
grid over a terrain thumbnail) are not chrome: they live in
`crates/hp-app/src/data_colors.rs`, the one file the test allows hued
literals in (any `data_colors.rs` of a UI crate is exempt, and a second one
fails the test `data_colors_live_in_one_file`). Chrome never reads from it.

## Translations

Every text of the UI is a translation key with English and French texts,
`(key, english, french)` rows in a `TrTable` (`hp_app::i18n`). Tables are
registered at startup; `t!("key")` translates, `tf!("key", a, b)` fills
`{0}`, `{1}`. Rules (tested by `i18n::lint_table` in each crate): no empty
text, no duplicate key, never the em dash or en dash characters; sentence
case, "..." for entries that open a dialog, French uses a space before `:`,
`?` and `!`.

Where each package puts its texts:

| Texts | Table |
|---|---|
| action labels `action.<id>`, tool names `tool.<x>`, submenu titles `menu.<x>` | `TR` of `crates/hp-app/src/actions/<package>.rs` |
| undo step names of new editing operations, `shell.undo.<label in snake case>` (the label "Move to floor" gives `shell.undo.move_to_floor`) | the same `TR` of `crates/hp-app/src/actions/<package>.rs` |
| shell texts (pages, segments, menu extensions in hp-ui-shell) | the `TR` of the module that uses them, keys `shell.<area>.`; module tables are listed in `tr.rs` `TABLES`, `settings_dialog::TABLES` and `status::TABLES` |
| binary texts of a dispatch module | the module's table, returned by `Package::translations` |
| other crates | that crate's own tables (see their sections) |

The hp-app tables (`strings::TR`, `actions::TABLES`) are loaded with the
registry itself, so every crate's tests see them without registering
anything; that is why undo step names of new operations go there and not in
`hp-ui-shell/src/tr.rs` (which keeps the names of the existing steps). The
test `every_undo_label_of_every_crate_is_translated` (hp-ui-shell) scans
every crate for undo labels and fails on a missing key.

Keys are unique across all tables of all crates (a later table silently
overrides an earlier key): prefix keys with your area and search the
workspace before adding a prefix. `hp-ui-shell` keys start with `shell.`
(tested); its tests also check every `"shell.` key literal of its sources
(subfolders included) has a row.

## Storage rows

Settings > Storage and the MCP tools `get_storage_usage` and `delete_storage`
list everything Hammer Pro writes on disk from one table, `TABLE` in
`crates/hp-app/src/storage/locations.rs`. A feature that writes a new cache,
output or temporary folder adds a `Spec` there (id, two translation keys,
risk, the `Part`s: a root among config, cache, autosave, logs, crash or temp,
a relative folder and a filter) and the two `storage.row.<id>` strings in
`storage/mod.rs`. Anything left under the cache folder shows up as "Other
cached data" until it has its row. Deletion goes through `AppDirs::guard`
(the item must resolve inside the folder of its root, links are removed and
never followed), so nothing outside Hammer Pro's own folders can be deleted,
and `in_use` names the files that are kept while the application uses them.

## MCP tools

AI apps edit the map through the tools of the MCP server
([MCP.md](MCP.md)). Every editor action is already reachable without new
code (`list_actions` and `run_action` read the actions registry), and so is
every setting (`get_settings`, `set_setting`), so a new feature needs MCP
work only for operations that deserve a semantic tool. To add one:

1. Describe it in the `SPECS` of an area file
   `crates/hammerpro/src/dispatch/mcp/tools_<area>.rs` (name, title,
   description for the model, `ToolKind`, JSON input schema in the subset
   every client reads: no `oneOf`, no type lists; `schema.rs` has helpers),
   and register the file's `DOMAIN` (category, specs, `run`) in `DOMAINS`
   of `dispatch/mcp/tools.rs`. The core tools stay in
   `crates/hp-mcp/src/tools.rs` (`EDITOR_TOOLS`).
2. Run it in the file's `run`: check every argument with `args(..)`
   (unknown names are refused), make edits with
   `edit(state, "<tool>", |doc| ..)` (one undo step "MCP: <tool>"), return
   a `ToolResult` (JSON, text or PNG). Work that needs the application
   returns an `AppOp` or an `AppFn`; work answered frames later (input,
   screenshots) returns `Outcome::Ui` (`automation.rs`).
3. Edits: add `shell.undo.mcp_<tool>` to `crates/hp-app/src/actions/mcp.rs`.
4. Regenerate the tool list of docs/MCP.md:
   `HP_UPDATE_MCP_DOCS=1 cargo test -p hammerpro docs_list_every_tool`.

Tests check every tool has a schema and an implementation, that failing
calls leave no undo step, that undo labels are translated, and
`harness.rs` calls every tool through the protocol with its example (from
`get_capabilities`; add one to `EXAMPLES` of `tools_help.rs` when the one
built from the schema is not useful) and with invalid arguments.

Actions that close the app, publish, start the game or a program, or show
a file dialog are listed in `NEEDS_CONFIRM` of `tools_actions.rs`: add new
ones there. Settings holding paths, programs or command lines are read only
for MCP (`read_only` in `tools_settings.rs`, by name).


## Lighting preview and lightmaps

The baked lighting preview (package 10) lives in
`crates/hammerpro/src/dispatch/lighting/`: it compiles the map in memory
with the Hammer Pro compiler (`hp_bspc::compile`), lights it with the
compiler's lighting step (`hp_bspc::rad`) in a child process
(`hammerpro --light-preview`, killed when a bake stops), and draws the
compiled faces with their lightmaps through
`crates/hp-render/src/lightmap.rs` (its pass hook is listed in
`extra_passes.rs`). Each compiled face is bound to the editor solid it
comes from (face ids lump, plane, bounds). What other packages need to know:

- `Renderer::upsert_object` and `remove_object` call
  `lightmap.object_changed(id)`: the lit faces of a changed object are
  hidden at once (they would show the old geometry) until the next bake. Keep these calls when reworking object uploads;
  `clear_objects` calls `objects_cleared`.
- 3D views in the `Baked` render mode get `RenderMode::Lit` (unbaked faces
  are lit by the preview lights), `LightmapGrid` gets
  `RenderMode::Flat`; the hook draws the lit or grid copy of the
  lightmapped faces over the opaque world (`hp_app::AppState::render_mode`).
- The props of the `Baked` mode are lit like the game: the child process
  sends the lit BSP back with the lightmaps, `health::portals::bake_prop_lighting`
  reads its ambient cubes, world lights and static prop vertex lighting
  (`-StaticPropLighting`, the compile setting) like the player view, and
  `AssetsApi::set_baked_prop_lighting` gives them to the assets, which
  light each model instance like the player view does
  (`play_model_lighting`). The renderer uses those records in the views
  showing the baked faces (`OPT_MODEL_LIGHT`, at the brightness of the
  baked faces); without a record a model keeps the preview lights.
- `hp_editor::luxels` gives the VBSP luxel math (`LuxelAxes`, `LuxelGrid`),
  the lightmap budget (`Document::lightmap_budget`) and the Hammer++
  `light_plus` options (`Document::light_plus`, `set_light_plus`).
- `hp_light::lights` parses lights the way VRAD does (attenuation, reach,
  spot cones); `hp-assets/src/lights.rs` uses it for the Lit preview: each
  `hp_render::PreviewLight` carries VRAD's constant, linear and quadratic
  terms (`LightFalloff`, with the spot `_exponent`), evaluated by the shader
  like `Light::falloff` (spot cones linear between the cones like VRAD),
  and texlight faces become preview lights. `light_dynamic` gives a preview
  light like the engine's dynamic light. Point and spot lights inside an
  opaque world brush are left out (VRAD gives them no visibility). The sun's
  shadow map leaves out the sky faces (the sun comes in through them) and
  the objects flagged `ObjectFlags::NO_SHADOW` (props with
  `disableshadows`); alpha tested props shadow by their alpha.
  `MaterialFlags::SELF_ILLUM` (`$selfillum` by the base alpha) glows in the
  lit views.
