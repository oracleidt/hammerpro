use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("array.title", "Array duplicate", "Duplication en série"),
    (
        "array.intro",
        "Duplicates the selected brushes and entities along the checked axes, with a gap between the bounding boxes.",
        "Duplique les brushes et entités sélectionnés le long des axes cochés, avec un espace entre les boîtes englobantes.",
    ),
    ("array.copies", "Copies", "Copies"),
    ("array.gap", "Gap", "Espace"),
    ("array.step", "Step", "Pas"),
    (
        "array.axis_tip",
        "Axis of the active grid (X, Y and Z of the world when the grid is not rotated)",
        "Axe de la grille active (X, Y et Z du monde quand la grille n'est pas tournée)",
    ),
    ("array.total", "Total: {0} new copies", "Total : {0} nouvelles copies"),
    (
        "array.grid_on",
        "Steps and positions are aligned to the active grid (rotated and custom grids included).",
        "Les pas et les positions sont alignés sur la grille active (grilles tournées et personnalisées comprises).",
    ),
    (
        "array.grid_off",
        "Snap to grid is off: the gap is used exactly.",
        "L'aimantation à la grille est désactivée : l'espace est utilisé tel quel.",
    ),
    ("array.too_many", "At most {0} copies at once.", "{0} copies au maximum à la fois."),
    ("array.err_nothing", "Select objects first.", "Sélectionnez d'abord des objets."),
    ("array.err_failed", "Array failed: {0}", "La duplication en série a échoué : {0}"),
    ("array.done", "Array: {0} copies created", "Duplication en série : {0} copies créées"),
    ("array.apply", "Duplicate", "Dupliquer"),
];
