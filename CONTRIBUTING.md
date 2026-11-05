# Contributing to Hammer Pro

Thanks for helping! This page explains how the project is organized and what
we expect from a pull request.

## Setup

- Install Rust (stable, 1.95 or newer) with `rustup`.
- Garry's Mod is optional for development, but many integration tests use it
  when present. Set `HAMMERPRO_GMOD_DIR` if it lives in a custom place.
- `cargo run` starts the editor. `cargo run -- path/to/map.vmf` opens a map.

## Before opening a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs the same commands on Windows and Linux.

## Code guidelines

- Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first. Keep crates focused
  and dependencies flowing in one direction.
- Libraries never panic on bad input: return errors.
- Keep the UI thread free: heavy work runs on worker threads and reports back
  through channels.
- Every user-visible string goes through `hp_app::i18n` with an English and a
  French translation in the crate's `TR` table. Run the crate tests: each table
  is linted for missing translations.
- Do not use the em dash or en dash characters anywhere (code, comments, UI
  text, docs). CI rejects them.
- The UI is black and white: take every UI color from the `hp_app::theme`
  tokens (see "Look and colors" in [docs/EXTENDING.md](docs/EXTENDING.md)).
  Color is for content only (the views, thumbnails, entity colors).
- The product name is written "Hammer Pro" in everything users read; crate,
  binary, folder and file names stay `hammerpro`.
- Add tests for new behaviour. Tests that need game files must skip themselves
  when the files are missing.

## Translations

Each UI crate has a `TR` table of `(key, English, French)` entries. Russian
translations are bundled by key in `crates/hp-app/src/i18n/ru.json`. Add the
Russian entry whenever adding a UI key, preserving every numbered placeholder
and technical identifier. The application's translation coverage test checks
that every registered UI string has Russian text and matching arguments.

Documentation lives in `docs/en`, `docs/fr` and `docs/ru`. Keep code examples,
command flags and API names unchanged when translating prose, and preserve
links and anchors. The documentation command follows the selected UI language.

## Reporting bugs

Please include the Hammer Pro version, your GPU, and if possible the VMF that
shows the problem. Crash reports help a lot: after a crash, the next start
shows where the report file is (`%APPDATA%\HammerPro\crash` on Windows) with
buttons to show it in its folder or copy its text into your message. Errors of
worker threads and of the graphics card are in the console panel and in the
log file (`%LOCALAPPDATA%\HammerPro\logs\hammerpro.log`).
