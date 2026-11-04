use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    (
        "blender.model_missing",
        "The model {0} was not found in the game content",
        "Le modèle {0} est introuvable dans le contenu du jeu",
    ),
    ("blender.model_bad", "Cannot read the model {0}: {1}", "Impossible de lire le modèle {0} : {1}"),
    (
        "blender.empty",
        "Nothing to export: the selection has no visible face (tool textures are left out)",
        "Rien à exporter : la sélection n'a aucune face visible (les textures d'outils sont exclues)",
    ),
    (
        "blender.nothing",
        "Select brushes, displacements or a prop with a model first",
        "Sélectionnez d'abord des brushes, des displacements ou un prop avec un modèle",
    ),
    (
        "blender.no_content",
        "The game content is not loaded (Settings > Game)",
        "Le contenu du jeu n'est pas chargé (Paramètres > Jeu)",
    ),
    (
        "blender.no_link",
        "No link with Blender: use Send to Blender first",
        "Aucun lien avec Blender : utilisez d'abord Envoyer vers Blender",
    ),
    ("blender.read_failed", "Cannot read the file from Blender: {0}", "Impossible de lire le fichier de Blender : {0}"),
    (
        "blender.link_changed",
        "The linked objects changed: send the selection to Blender again",
        "Les objets liés ont changé : renvoyez la sélection vers Blender",
    ),
    (
        "blender.pulled_prop",
        "Blender update: {0} rebuilt, {1} props refreshed ({2} triangles)",
        "Mise à jour de Blender : {0} reconstruit, {1} props rafraîchis ({2} triangles)",
    ),
    (
        "blender.pulled_brushes",
        "Blender update: {0} brushes made ({1} textured faces) in place of {2}",
        "Mise à jour de Blender : {0} brushes créés ({1} faces texturées) à la place de {2}",
    ),
    (
        "blender.pulled_as_prop",
        "Blender update: placed as the prop {0} because it cannot become brushes ({1})",
        "Mise à jour de Blender : placé comme le prop {0} car il ne peut pas devenir des brushes ({1})",
    ),
    (
        "blender.pulled_disp",
        "Blender update: {0} displacements updated ({1} vertices moved), {2} reshaped to the new mesh, {3} created, {4} skipped",
        "Mise à jour de Blender : {0} displacements mis à jour ({1} sommets déplacés), {2} remodelés sur le nouveau maillage, {3} créés, {4} ignorés",
    ),
    (
        "blender.too_many_disps",
        "{0} displacements are too many for one send (at most {1})",
        "{0} displacements, c'est trop pour un envoi (au plus {1})",
    ),
    ("blender.pull_failed", "Blender update failed: {0}", "La mise à jour de Blender a échoué : {0}"),
    ("blender.busy", "An update from Blender is already running", "Une mise à jour depuis Blender est déjà en cours"),
    ("blender.launch_failed", "Cannot start Blender ({0}): {1}", "Impossible de lancer Blender ({0}) : {1}"),
    ("blender.sandboxed", "Automated run: Blender is not started", "Exécution automatique : Blender n'est pas lancé"),
    ("blender.not_found", "Blender was not found", "Blender est introuvable"),
    ("blender.sent", "Sent to Blender: {0}", "Envoyé vers Blender : {0}"),
    ("blender.exported", "Exported to {0}", "Exporté vers {0}"),
    ("blender.stopped", "The Blender live link is stopped", "Le lien direct avec Blender est arrêté"),
    (
        "blender.addon_installed",
        "Blender add-on written to {0}. It is also loaded each time Hammer Pro starts Blender; to keep it always on, enable Hammer Pro Bridge in Edit > Preferences > Add-ons",
        "Add-on Blender écrit dans {0}. Il est aussi chargé à chaque lancement de Blender par Hammer Pro ; pour l'avoir en permanence, activez Hammer Pro Bridge dans Edit > Preferences > Add-ons",
    ),
    (
        "blender.addon_written",
        "No Blender folder found: the add-on was written to {0}. In Blender use Edit > Preferences > Add-ons > Install from disk",
        "Aucun dossier Blender trouvé : l'add-on a été écrit dans {0}. Dans Blender, utilisez Edit > Preferences > Add-ons > Install from disk",
    ),
    ("blender.missing_title", "Blender not found", "Blender introuvable"),
    (
        "blender.missing_body",
        "Hammer Pro could not find Blender. Browse to blender.exe, or install Blender from blender.org.",
        "Hammer Pro n'a pas trouvé Blender. Choisissez blender.exe, ou installez Blender depuis blender.org.",
    ),
    ("blender.exported_to", "The selection was exported to {0}.", "La sélection a été exportée vers {0}."),
    ("blender.browse", "Browse...", "Parcourir..."),
    ("blender.retry", "Start Blender", "Lancer Blender"),
    ("blender.close", "Close", "Fermer"),
    ("blender.pick_title", "Choose the Blender program", "Choisir le programme Blender"),
    ("blender.wrong_exe", "{0} is not a Blender program", "{0} n'est pas un programme Blender"),
    ("blender.path_saved", "Blender found: {0}", "Blender trouvé : {0}"),
];
