[Documentation](../README.md) > Development > Building from source | [Français](../../fr/development/building.md) | [Русский](../../ru/development/building.md)

# Building from source

Hammer Pro is a Rust workspace of small crates. This page explains how to build the editor, run it and run the tests.

## Requirements

- **Rust 1.95 or newer** (stable), installed with `rustup`. The workspace uses the 2024 edition.
- On Windows, the MSVC toolchain (the default of `rustup` on Windows). The build embeds the application icon itself: no resource compiler is needed.
- Garry's Mod is optional: many integration tests use it when it is present and skip themselves otherwise.

The map compiler is part of the workspace: Valve's tools are needed neither to build Hammer Pro nor to compile maps.

## Building the editor

```bash
git clone <repository URL>
cd HammerPro
cargo build --release -p hammerpro
```

The executable is `target/release/hammerpro.exe` (`target/release/hammerpro` on Linux). `hammerpro` is the default member of the workspace, so `cargo build --release` alone builds the same thing.

To run it from the sources:

```bash
cargo run                       # starts the editor (dev profile)
cargo run -- path/to/map.vmf    # opens a map
cargo run --release             # release speed
```

The build records its date and, in a git checkout, the commit, both shown in **Help > About Hammer Pro**. `SOURCE_DATE_EPOCH` is honored for reproducible builds.

## Profiles

| Profile | Command | Use | Settings |
|---|---|---|---|
| `dev` | `cargo build`, `cargo run`, `cargo test` | Everyday development. Your code at `opt-level = 1`, every dependency at `opt-level = 3`, so the editor stays usable. Line tables only (enough for backtraces), no incremental cache. | `opt-level = 1`, `debug = "line-tables-only"`, `incremental = false` |
| `release` | `cargo build --release` | The builds you ship. | `lto = "thin"`, `codegen-units = 1`, no debug info |
| `fast` | `cargo build --profile fast`, `cargo test --profile fast` | Iteration builds: release speed at run time, much faster and lighter to build than `release`. Output in `target/fast/`. | Inherits `release`, `lto = false`, `codegen-units = 16`, `incremental = true` |

## Tests

```bash
cargo test --workspace
```

- Tests that use real game data look for Garry's Mod and skip themselves when it is not installed. Set `HAMMERPRO_GMOD_DIR` to the Garry's Mod folder (the one holding `garrysmod` and `bin`) when it is not in the place the tests expect. The GMA tests read Workshop items from `HAMMERPRO_WORKSHOP_DIR`.
- Each crate's translation table is checked for missing translations by the crate's tests.
- Before a pull request, run the same checks as CI:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Build folders

A full build folder takes several gigabytes. To put it on another drive, set `CARGO_TARGET_DIR`:

```bat
set CARGO_TARGET_DIR=E:\builds\hammerpro
cargo build --release -p hammerpro
```

The executable is then in `E:\builds\hammerpro\release\`. Profiles keep separate folders (`debug`, `release`, `fast`), so switching between them does not rebuild everything.

## Patched dependency

`vendor/wgpu-hal` is wgpu-hal 30.0.1 with one change, used through `[patch.crates-io]` in the workspace `Cargo.toml`: on Windows with Vulkan, the wait for a swapchain image to come back from the screen lasts up to 30 seconds instead of one, because a longer wait was treated as a lost device (a busy graphics card, another program or a long frame closed the editor). When wgpu is upgraded, check whether the new version still turns that timeout into a lost device; if not, remove the folder and the patch.

## Linux

The workspace builds on Linux too (CI runs the checks on Windows and Linux). The window supports both Wayland and X11. Paths follow the usual folders: settings in `~/.config/HammerPro`, logs and autosaves in `~/.local/share/HammerPro`, caches in `~/.cache/HammerPro`.

## FAQ

**Which crate do I build to get the editor?** `hammerpro` (`crates/hammerpro`), the default member.

**The build is slow.** Use the `fast` profile while iterating, and `--release` only for builds you share.

**The tests take long or fail for missing game files.** Tests that need game files skip themselves when the files are missing; point `HAMMERPRO_GMOD_DIR` to your install to run them.

## Related pages

- [Architecture](architecture.md)
- [Contributing](contributing.md)
- [Extending Hammer Pro](extending.md)
- [Installation](../getting-started/installation.md)

---
[Back to the documentation index](../README.md)
