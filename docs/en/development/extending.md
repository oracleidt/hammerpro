[Documentation](../README.md) > Development > Extending Hammer Pro | [Français](../../fr/development/extending.md)

# Extending Hammer Pro

Hammer Pro is built from feature packages that plug into shared registries instead of editing each other's files. This page is a short map of those extension points. The complete developer guide, with the code of every hook, is [docs/EXTENDING.md](../../EXTENDING.md) (English, written for contributors).

## The rule

Work in your own files. When a host file belongs to another package, plug in through a hook. Each package has a key used in file names: shell, selection (1), brush (2), workflow (3), instances (4), visibility (5), helpers (6), mesh (7), entities (8), render (9), lighting (10), placement (11), compile (12), startup (13), launch (14), content (15), health (16), lab (17) and mcp.

## Extension points

| Extension point | What it is for | Where |
| --- | --- | --- |
| Actions | Every command (menus, toolbar, shortcuts, command palette, `F1` reference, keyboard settings, scripts, MCP). One enum, declared in one file per package with `action_list!`: id, menu, menu group, default shortcut, status. | `crates/hp-app/src/actions/<package>.rs` |
| Menus | Placement of an action in a menu or submenu (`MenuGroup`), sorted by section and order. | `crates/hp-app/src/actions/groups.rs` |
| Dispatch | The code that runs an action. | `crates/hammerpro/src/dispatch/<package>.rs` |
| Settings | One struct per package, stored as its own TOML table, every field with a default. | `crates/hp-app/src/settings/<package>.rs` |
| Settings pages | Pages of the settings window. | `crates/hp-ui-shell/src/settings_dialog/<package>.rs` |
| Status bar | Segments of the status bar. | `crates/hp-ui-shell/src/status/` |
| Sidecar | Per map data kept outside the VMF. | `crates/hp-app/src/sidecar.rs` |
| Overlay layers | Geometry drawn by the views on top of the scene. | `crates/hp-ui-viewports/src/layers/` |
| Scene producers | Objects added to the rendered scene. | see EXTENDING.md |
| Tool hooks and picking | Tools reacting to input, picking in the views. | see EXTENDING.md |
| Editor extension | Extra VMF blocks (Hammer++ `*_plus` blocks are kept this way). | `crates/hp-editor` |
| Compile and launch | Steps of the compile pipeline, launch arguments. | `crates/hp-compile`, `crates/hp-ui-mapping/src/compile/` |
| Translations | Every UI text as `(key, English, French)` rows of a `TR` table. | each crate |
| MCP tools | Semantic tools of the MCP server. Every action and setting is already reachable through `run_action` and `set_setting`. | `crates/hammerpro/src/dispatch/mcp/tools_<area>.rs` |

## Translations

Every user-visible text goes through the i18n layer with an English and a French text. Keys are prefixed with the area (`shell.`, `tex.`, `disp.`...) and must be unique across all crates. The table lint rejects empty texts, duplicate keys and the em dash or en dash characters. French uses a space before `:`, `?` and `!`. Adding a language means adding a variant to `hp_app::i18n::Lang` and a column to the tables.

## Look and colors

The interface is black and white: every UI color comes from the `hp_app::theme` tokens. Color is reserved for content (views, thumbnails, entity colors).

## Adding an MCP tool

1. Describe the tool in the `SPECS` of an area file `crates/hammerpro/src/dispatch/mcp/tools_<area>.rs`.
2. Implement it in that file's `run`, with every edit made through one undo step named "MCP: <tool>".
3. Add the undo label `shell.undo.mcp_<tool>` to `crates/hp-app/src/actions/mcp.rs`.
4. Regenerate the tool list of `docs/MCP.md` with `HP_UPDATE_MCP_DOCS=1 cargo test -p hammerpro docs_list_every_tool`.

## Note

[docs/EXTENDING.md](../../EXTENDING.md) was written while the packages were being built: its tables still call some actions "Planned". In 1.0.2 every action listed there is shipped (see [Actions and keyboard shortcuts](../reference/actions-shortcuts.md)).

## Related pages

- [Architecture](architecture.md)
- [Building from source](building.md)
- [Contributing](contributing.md)
- [MCP tool reference](../../MCP.md)

---
[Back to the documentation index](../README.md)
