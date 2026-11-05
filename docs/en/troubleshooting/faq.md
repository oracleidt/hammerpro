[Documentation](../README.md) > Troubleshooting > FAQ | [Français](../../fr/troubleshooting/faq.md) | [Русский](../../ru/troubleshooting/faq.md)

# FAQ

Short answers to the questions asked most often, with links to the pages that explain more.

## General

**What is Hammer Pro?** A level editor for Source engine maps, built for Garry's Mod first. It opens and saves standard `.vmf` files, renders with the GPU and compiles maps with its own built in compiler. See [Installation](../getting-started/installation.md).

**Is it free?** Yes. Hammer Pro is free and open source software under the MIT license (see [LICENSE](../../../LICENSE)).

**Is Hammer Pro made by Valve or Facepunch?** No. It is not affiliated with Valve or Facepunch Studios, and it does not include or redistribute any Valve code or game content.

**Does it replace Hammer and Hammer++?** It does what they do and more, but you do not have to choose: the same `.vmf` opens in all three, and Hammer++ data is kept. See [VMF compatibility](../getting-started/vmf-compatibility.md).

**Which games can I map for?** Garry's Mod first. Many other Source games are supported as compile targets (Half-Life 2 and its episodes, Counter-Strike: Source, Team Fortress 2, Portal, Left 4 Dead 2, Black Mesa and more). See [Target games](../compiler/target-games.md).

**Which systems are supported?** Windows 10 and 11 (64-bit) with a GPU supporting Vulkan, DirectX 12 or Metal. Linux builds compile from source.

## Setup

**Do I need VBSP, VVIS and VRAD?** No. Hammer Pro compiles maps itself. See [Compiler overview](../compiler/overview.md).

**Hammer Pro does not find Garry's Mod.** **Tools > Options... > Game**, then **Detect** or **Browse...** to the folder holding `garrysmod` and `bin`. See [Game setup](../getting-started/game-setup.md).

**I come from Hammer: can I keep my shortcuts?** Yes. Choose **Hammer** or **Hammer++** in the setup assistant (**Help > Setup assistant...**) or in **Settings > Keyboard**. See [Keyboard shortcuts](../features/keyboard-shortcuts.md).

**How do I switch between English, French and Russian?** **View > Language**, at any time; the whole interface switches at once. `--lang en` or `--lang fr` on the command line forces it for one run. See [Themes and languages](../features/themes-languages.md).

**Where are my settings?** In `%APPDATA%\HammerPro\settings.toml`. See [Files and folders](../reference/files-and-folders.md).

## Mapping

**My textures from the Workshop or an addon do not show.** Check **Mount addons** and **Mount Workshop downloads** in **Settings > Game**, then **Apply and reload content**.

**Can I use my own PNG or JPG textures?** Yes: add their folder in **Settings > Game > Custom texture folders**. Images work as materials at once and are converted to VTF + VMT when you compile. See [Custom texture folders](../features/custom-texture-folders.md).

**Can I open several maps?** Yes, each in its own tab; copy and paste works between them and even between two Hammer Pro windows. See [Several open maps](../features/multiple-maps.md).

**I deleted something by mistake.** `Ctrl+Z`. Every edit is undoable, up to 500 steps by default. See [Undo and history](../features/undo-history.md).

## Compiling and playing

**How do I test my map?** Save it, then press `F9`: it compiles and Garry's Mod starts on the map. See [Your first map](../getting-started/first-map.md) and [Compile and test in Garry's Mod](../guides/compile-and-test-gmod.md).

**My map leaks.** Follow the leak path drawn in the views and seal the hole with a world brush. See [Common errors](common-errors.md).

**Players see purple and black checkerboards.** Custom content was not shipped with the map. See [Common errors](common-errors.md) and [Build and save as content](../features/build-content-release.md).

## AI apps

**Can an AI assistant edit my map?** Yes, through the built in MCP server (off by default, switched on per window with the **MCP** pill). Every AI edit is one undo step. See [MCP server](../reference/mcp-server.md) and [Use Hammer Pro with an AI assistant](../guides/mcp-with-ai-assistant.md).

## Problems

**Hammer Pro crashed.** Your changes are usually saved: see [Crashes and logs](crashes-and-logs.md).

**The editor is slow.** See [Editor performance](performance.md).

**My Discord status does not show what I map.** See [Discord presence not showing](discord-presence.md).

**How do I report a bug?** See [Reporting bugs](reporting-bugs.md).

## Related pages

- [Common errors](common-errors.md)
- [Crashes and logs](crashes-and-logs.md)
- [Editor performance](performance.md)
- [Reporting bugs](reporting-bugs.md)

---
[Back to the documentation index](../README.md)
