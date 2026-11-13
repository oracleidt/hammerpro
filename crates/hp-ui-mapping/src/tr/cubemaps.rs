use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("cubemaps.title", "Cubemaps", "Cubemaps"),
    (
        "cubemaps.intro",
        "Where reflections come from: the rooms with reflective materials, the env_cubemap each face takes, and where cubemaps are missing. Nothing changes until you accept a suggestion.",
        "D'où viennent les reflets : les pièces avec des matériaux réfléchissants, l'env_cubemap que prend chaque face, et où il manque des cubemaps. Rien ne change tant que vous n'acceptez pas une suggestion.",
    ),
    ("cubemaps.analyze", "Analyze", "Analyser"),
    (
        "cubemaps.analyze_tip",
        "Look at the reflective faces, the rooms and the cubemaps of the map again",
        "Examiner à nouveau les faces réfléchissantes, les pièces et les cubemaps de la map",
    ),
    (
        "cubemaps.view_tip",
        "Colour each face by the cubemap it will use and draw a line from the reflective faces to their cubemap (red: another room or through a wall)",
        "Colorer chaque face selon le cubemap qu'elle utilisera et tracer une ligne des faces réfléchissantes vers leur cubemap (rouge : une autre pièce ou à travers un mur)",
    ),
    ("cubemaps.areas", "Use compiler areas", "Utiliser les zones du compilateur"),
    (
        "cubemaps.areas_tip",
        "Compile the visibility in the background (like Map > Optimize) so rooms follow the areas cut by areaportals, then analyze again",
        "Compiler la visibilité en arrière-plan (comme Map > Optimiser) pour que les pièces suivent les zones découpées par les areaportals, puis analyser à nouveau",
    ),
    ("cubemaps.areas_running", "Compiling the areas...", "Compilation des zones..."),
    ("cubemaps.none", "No analysis yet: press Analyze.", "Pas encore d'analyse : appuyez sur Analyser."),
    (
        "cubemaps.stale",
        "The map changed since this analysis: press Analyze.",
        "La map a changé depuis cette analyse : appuyez sur Analyser.",
    ),
    (
        "cubemaps.summary",
        "{0} cubemaps (about {1} MB), {2} reflective faces in {3} rooms",
        "{0} cubemaps (environ {1} Mo), {2} faces réfléchissantes dans {3} pièces",
    ),
    (
        "cubemaps.summary_wrong",
        "{0} faces take the cubemap of another room ({1} through a wall), {2} faces are far from their cubemap",
        "{0} faces prennent le cubemap d'une autre pièce ({1} à travers un mur), {2} faces sont loin de leur cubemap",
    ),
    (
        "cubemaps.unfixable",
        "{0} faces cannot get a better cubemap even with the suggestions: no free spot near them sees them, or their group has too little reflective surface.",
        "{0} faces ne peuvent pas avoir un meilleur cubemap même avec les suggestions : aucun endroit libre proche ne les voit, ou leur groupe a trop peu de surface réfléchissante.",
    ),
    (
        "cubemaps.areas_used",
        "Rooms follow the areas of the last visibility compile.",
        "Les pièces suivent les zones de la dernière compilation de visibilité.",
    ),
    (
        "cubemaps.areas_unused",
        "Rooms come from the geometry (lines of sight and openings).",
        "Les pièces viennent de la géométrie (lignes de vue et ouvertures).",
    ),
    ("cubemaps.suggestions", "Suggestions ({0})", "Suggestions ({0})"),
    ("cubemaps.size", "Size of new cubemaps:", "Taille des nouveaux cubemaps :"),
    ("cubemaps.size_suggested", "As suggested", "Comme suggéré"),
    ("cubemaps.size_default", "Default ({0}x{0})", "Par défaut ({0}x{0})"),
    (
        "cubemaps.no_suggestions",
        "Nothing to suggest: every room with reflective faces has its cubemap.",
        "Rien à suggérer : chaque pièce avec des faces réfléchissantes a son cubemap.",
    ),
    ("cubemaps.all_dismissed", "Every suggestion was dismissed.", "Toutes les suggestions ont été écartées."),
    ("cubemaps.accept_all", "Accept all", "Tout accepter"),
    (
        "cubemaps.accept_all_tip",
        "Place every suggested cubemap and sides change in one undo step",
        "Placer tous les cubemaps et changements de sides suggérés en une seule étape d'annulation",
    ),
    ("cubemaps.dismiss_all", "Dismiss all", "Tout écarter"),
    ("cubemaps.accept", "Accept", "Accepter"),
    ("cubemaps.dismiss", "Dismiss", "Écarter"),
    ("cubemaps.findings", "Findings ({0})", "Constats ({0})"),
    ("cubemaps.clean", "No cubemap problem found.", "Aucun problème de cubemap trouvé."),
    (
        "cubemaps.suggest_new",
        "New env_cubemap at {0}, at eye height, for {1} reflective faces",
        "Nouvel env_cubemap en {0}, à hauteur des yeux, pour {1} faces réfléchissantes",
    ),
    (
        "cubemaps.suggest_new_sides",
        "New env_cubemap at {0}, at eye height, for {1} reflective faces, with {2} faces in its sides",
        "Nouvel env_cubemap en {0}, à hauteur des yeux, pour {1} faces réfléchissantes, avec {2} faces dans ses sides",
    ),
    (
        "cubemaps.suggest_sides",
        "Add {0} faces of its room to the sides of env_cubemap {1}: they take a cubemap of another room now",
        "Ajouter {0} faces de sa pièce aux sides de l'env_cubemap {1} : elles prennent le cubemap d'une autre pièce actuellement",
    ),
    (
        "cubemaps.accepted",
        "{0} cubemaps placed or updated (one undo step).",
        "{0} cubemaps placés ou mis à jour (une étape d'annulation).",
    ),
    ("cubemaps.accept_failed", "The cubemaps could not be placed: {0}", "Les cubemaps n'ont pas pu être placés : {0}"),
];
