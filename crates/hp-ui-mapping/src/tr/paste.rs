use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("paste.title", "Paste special", "Collage spécial"),
    ("paste.copies", "Copies", "Copies"),
    ("paste.offset", "Offset per copy", "Décalage par copie"),
    ("paste.rotation", "Rotation per copy", "Rotation par copie"),
    ("paste.name_prefix", "Name prefix", "Préfixe de nom"),
    ("paste.name_prefix_hint", "optional", "facultatif"),
    ("paste.start_at_original", "Start at the original position", "Commencer à la position d'origine"),
    (
        "paste.start_at_original_tip",
        "The first copy is placed exactly over the copied objects",
        "La première copie est placée exactement sur les objets copiés",
    ),
    ("paste.group", "Group each copy", "Grouper chaque copie"),
    ("paste.unique_names", "Make entity names unique", "Rendre les noms d'entités uniques"),
    (
        "paste.unique_names_tip",
        "Rename pasted entities whose name already exists (name_2, name_3...)",
        "Renommer les entités collées dont le nom existe déjà (nom_2, nom_3...)",
    ),
    ("paste.total_offset", "Offset of the last copy: {0}", "Décalage de la dernière copie : {0}"),
    (
        "paste.preview_ok",
        "Preview in the views: {0} object(s) x {1} copy(ies)",
        "Aperçu dans les vues : {0} objet(s) x {1} copie(s)",
    ),
    (
        "paste.preview_partial",
        "Preview in the views: first {0} of {1} copies",
        "Aperçu dans les vues : les {0} premières copies sur {1}",
    ),
    (
        "paste.size_tip",
        "Use the size of the clipboard contents on this axis (copies side by side)",
        "Utiliser la taille du contenu du presse-papiers sur cet axe (copies côte à côte)",
    ),
    ("paste.around_pivot", "Rotate around the pivot", "Pivoter autour du pivot"),
    (
        "paste.around_pivot_tip",
        "The rotation turns around the movable pivot at {0} instead of the center of the copied objects",
        "La rotation tourne autour du pivot déplaçable en {0} au lieu du centre des objets copiés",
    ),
    (
        "paste.around_pivot_off",
        "Show the movable pivot (Map menu) to rotate the copies around it",
        "Affichez le pivot déplaçable (menu Map) pour faire tourner les copies autour de lui",
    ),
    (
        "paste.many_copies",
        "{0} copies: pasting can take a while and makes the map much heavier.",
        "{0} copies : le collage peut prendre du temps et alourdit beaucoup la carte.",
    ),
    ("paste.paste", "Paste", "Coller"),
    (
        "paste.err_empty",
        "The clipboard does not contain map objects. Copy something first.",
        "Le presse-papiers ne contient pas d'objets de map. Copiez d'abord quelque chose.",
    ),
    ("paste.err_failed", "Paste failed: {0}", "Le collage a échoué : {0}"),
    ("paste.err_clipboard", "Could not read the clipboard: {0}", "Impossible de lire le presse-papiers : {0}"),
    ("paste.done", "Pasted {0} ({1})", "Collage : {0} ({1})"),
    ("paste.objects_one", "{0} object", "{0} objet"),
    ("paste.objects", "{0} objects", "{0} objets"),
    ("paste.copies_count_one", "{0} copy", "{0} copie"),
    ("paste.copies_count", "{0} copies", "{0} copies"),
];
