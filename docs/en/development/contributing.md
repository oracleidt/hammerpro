[Documentation](../README.md) > Development > Contributing | [Français](../../fr/development/contributing.md) | [Русский](../../ru/development/contributing.md)

# Contributing

Contributions are welcome: code, translations, bug reports and documentation. This page summarizes the rules; the reference is [CONTRIBUTING.md](../../../CONTRIBUTING.md) at the root of the repository (in English).

## Setup

- Install stable Rust 1.95 or newer with `rustup`.
- Garry's Mod is optional, but many integration tests use it when present. Set `HAMMERPRO_GMOD_DIR` if it lives in a custom place.
- `cargo run` starts the editor, `cargo run -- path/to/map.vmf` opens a map. See [Building from source](building.md).

## Before opening a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same commands on Windows and Linux.

## Code guidelines

| Rule | Why |
|---|---|
| Read [ARCHITECTURE.md](../../ARCHITECTURE.md) first; keep crates focused and dependencies flowing in one direction. | See [Architecture](architecture.md). |
| Libraries never panic on bad input: they return errors. | Maps and game files come from anywhere. |
| Keep the UI thread free: heavy work runs on worker threads and reports back through channels. | The editor must stay smooth on huge maps. |
| Every user visible string goes through `hp_app::i18n`, with an English and a French text in the crate's `TR` table. The crate tests check for missing translations. | The interface switches language at runtime. |
| No em dash or en dash characters anywhere (code, comments, UI text, docs). CI rejects them. | Project style. |
| Take every UI color from the `hp_app::theme` tokens. Color is for content only (views, thumbnails, entity colors). | The interface is black and white. |
| Write "Hammer Pro" in everything users read; crate, binary, folder and file names stay `hammerpro`. | One product name. |
| Add tests for new behaviour. Tests that need game files skip themselves when the files are missing. | CI has no game installed. |
| Actions and settings reach the MCP server without new code (`run_action`, `set_setting`); an operation that deserves its own MCP tool gets one, and the tool list of MCP.md is regenerated. | AI apps drive the whole editor (see [MCP.md](../../MCP.md) and the "MCP tools" section of [EXTENDING.md](../../EXTENDING.md)). |

New features plug into the registries of `hp-app` (actions, settings, menus, settings pages, status bar, overlays, sidecar data): see [Extending Hammer Pro](extending.md) and [EXTENDING.md](../../EXTENDING.md).

## Translations

Each UI crate has a `TR` table of `(key, English, French)` entries. To improve a translation, edit the text in that table. To add a language, add a variant to `hp_app::i18n::Lang` and a column to the tables.

## Reporting bugs

Include the Hammer Pro version, your GPU and, if possible, the VMF that shows the problem, plus the crash report and the log file. See [Reporting bugs](../troubleshooting/reporting-bugs.md).

## License

Hammer Pro is released under the MIT license (see [LICENSE](../../../LICENSE)).

## Related pages

- [CONTRIBUTING.md](../../../CONTRIBUTING.md)
- [Building from source](building.md)
- [Architecture](architecture.md)
- [Extending Hammer Pro](extending.md)

---
[Back to the documentation index](../README.md)
