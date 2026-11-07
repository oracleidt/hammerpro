# UI icon font

The font contains normalized Microsoft Fluent UI System Icons and the
`mesh` outline from Tabler Icons 3.50.0 (U+10201 in the upstream webfont).
The displacement symbol `🗻` maps to `mesh`; texture tools retain `image`.
Both upstream MIT notices are included in `LICENSE.txt` and the font metadata.

To regenerate, run `python scripts/build-ui-icons.py <source-directory>` with
FontTools available. The directory must contain `fluent.ttf`, `fluent.json`,
`fluent-license.txt`, `tabler.ttf`, and `tabler-license.txt`.

Sources: [Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons)
and [Tabler Icons](https://github.com/tabler/tabler-icons), whose pinned webfont
is distributed in `@tabler/icons-webfont@3.50.0/dist/fonts/tabler-icons.ttf`.
