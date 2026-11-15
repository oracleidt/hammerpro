use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("cordons.title", "Cordons", "Cordons"),
    (
        "cordons.new_tip",
        "New cordon around the selection (or at the center of the 2D views)",
        "Nouveau cordon autour de la sélection (ou au centre des vues 2D)",
    ),
    ("cordons.new_name", "Cordon", "Cordon"),
    ("cordons.use", "Use", "Utiliser"),
    (
        "cordons.use_tip",
        "Use cordons: objects outside every active cordon are dimmed, cannot be picked and are left out of cordon compiles",
        "Utiliser les cordons : les objets en dehors de tous les cordons actifs sont estompés, ne peuvent pas être sélectionnés et sont exclus des compilations du cordon",
    ),
    ("cordons.count_one", "{0} cordon", "{0} cordon"),
    ("cordons.count", "{0} cordons", "{0} cordons"),
    (
        "cordons.empty",
        "No cordons. Click + or draw one with the cordon tool.",
        "Aucun cordon. Cliquez sur + ou dessinez-en un avec l'outil cordon.",
    ),
    ("cordons.more", "{0} more cordons not listed", "{0} autres cordons non listés"),
    ("cordons.active_tip", "Include this cordon in the cordoned area", "Inclure ce cordon dans la zone cordonnée"),
    ("cordons.delete", "Delete cordon", "Supprimer le cordon"),
    ("cordons.focus", "Frame the views on this cordon", "Cadrer les vues sur ce cordon"),
    ("cordons.edit_box", "Edit the box with the cordon tool", "Modifier la boîte avec l'outil cordon"),
    ("cordons.rename", "Rename", "Renommer"),
    (
        "cordons.mode_include_tip",
        "Include cordon: only what touches the include cordons is kept. Click to make it an exclude cordon.",
        "Cordon d'inclusion : seul ce qui touche les cordons d'inclusion est gardé. Cliquez pour en faire un cordon d'exclusion.",
    ),
    (
        "cordons.mode_exclude_tip",
        "Exclude cordon: what lies fully inside it is dimmed and left out of cordon compiles. Click to make it an include cordon.",
        "Cordon d'exclusion : ce qui est entièrement dedans est estompé et retiré des compilations du cordon. Cliquez pour en faire un cordon d'inclusion.",
    ),
    ("cordons.make_exclude", "Make it an exclude cordon", "En faire un cordon d'exclusion"),
    ("cordons.make_include", "Make it an include cordon", "En faire un cordon d'inclusion"),
    (
        "cordons.row_tip",
        "From {0} to {1} (size {2}). Click to make it the cordon the cordon tool edits, double click to rename.",
        "De {0} à {1} (taille {2}). Cliquez pour en faire le cordon modifié par l'outil cordon, double-cliquez pour le renommer.",
    ),
    (
        "cordons.row_tip_empty",
        "This cordon has no volume: it is deleted at its next edit.",
        "Ce cordon n'a pas de volume : il sera supprimé à sa prochaine modification.",
    ),
    ("cordons.error", "Cordon error: {0}", "Erreur de cordon : {0}"),
    ("cordons.status", "Cordon", "Cordon"),
    (
        "cordons.status_tip_one",
        "Cordons are in use ({0} box): objects outside are dimmed. Click to stop using them.",
        "Les cordons sont utilisés ({0} boîte) : les objets en dehors sont estompés. Cliquez pour ne plus les utiliser.",
    ),
    (
        "cordons.status_tip",
        "Cordons are in use ({0} boxes): objects outside are dimmed. Click to stop using them.",
        "Les cordons sont utilisés ({0} boîtes) : les objets en dehors sont estompés. Cliquez pour ne plus les utiliser.",
    ),
];
