use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("lab.logic.title", "Logical view", "Vue logique"),
    ("lab.logic.search", "Find an entity...", "Chercher une entité..."),
    ("lab.logic.filter_all", "All entities", "Toutes les entités"),
    ("lab.logic.filter_named", "Named and connected", "Nommées et connectées"),
    ("lab.logic.filter_connected", "Connected only", "Connectées seulement"),
    ("lab.logic.frame_all", "Frame all", "Tout cadrer"),
    ("lab.logic.frame_all_tip", "Show every entity (Home)", "Afficher toutes les entités (Origine)"),
    ("lab.logic.frame_selection", "Frame selection", "Cadrer la sélection"),
    ("lab.logic.arrange", "Arrange", "Organiser"),
    (
        "lab.logic.arrange_tip",
        "Place every entity automatically and store the positions in the map (can be undone)",
        "Placer automatiquement toutes les entités et enregistrer les positions dans la map (annulable)",
    ),
    ("lab.logic.summary", "Entities: {0}, outputs: {1}", "Entités : {0}, sorties : {1}"),
    ("lab.logic.search_results", "Search: {0} shown", "Recherche : {0} affichée(s)"),
    ("lab.logic.problems", "Outputs with a problem: {0}", "Sorties à problème : {0}"),
    ("lab.logic.legend_valid", "valid", "valide"),
    ("lab.logic.legend_missing", "missing target", "cible introuvable"),
    ("lab.logic.legend_unknown", "unknown output or input", "sortie ou entrée inconnue"),
    (
        "lab.logic.empty",
        "No entity of this map has a name or an output",
        "Aucune entité de cette map n'a de nom ni de sortie",
    ),
    ("lab.logic.unnamed", "(unnamed)", "(sans nom)"),
    ("lab.logic.properties", "Properties", "Propriétés"),
    ("lab.logic.center_views", "Center the views on it", "Centrer les vues dessus"),
    ("lab.logic.unpin", "Use automatic position", "Utiliser la position automatique"),
    ("lab.logic.why_missing", "No entity has this name", "Aucune entité ne porte ce nom"),
    (
        "lab.logic.why_output",
        "This output is not in the entity definition of the source",
        "Cette sortie n'existe pas dans la définition de l'entité source",
    ),
    (
        "lab.logic.why_input",
        "This input is not in the entity definition of the target",
        "Cette entrée n'existe pas dans la définition de l'entité cible",
    ),
    ("lab.logic.new_output", "New output", "Nouvelle sortie"),
    ("lab.logic.output", "Output", "Sortie"),
    ("lab.logic.target", "Target", "Cible"),
    (
        "lab.logic.name_tip",
        "The target has no name: it gets this one",
        "La cible n'a pas de nom : elle recevra celui-ci",
    ),
    ("lab.logic.input", "Input", "Entrée"),
    ("lab.logic.param", "Parameter", "Paramètre"),
    ("lab.logic.delay", "Delay", "Délai"),
    ("lab.logic.once", "Only once", "Une seule fois"),
    ("lab.logic.after", " after {0} s", " après {0} s"),
    ("lab.logic.once_short", "(once)", "(une fois)"),
    ("lab.logic.add_failed", "The output could not be added: {0}", "Impossible d'ajouter la sortie : {0}"),
    ("lab.nav.loaded", "Nav mesh {0}: areas: {1}, ladders: {2}", "Nav mesh {0} : zones : {1}, échelles : {2}"),
    (
        "lab.nav.isolated",
        "Nav areas cut off from the main part of the mesh (where players spawn, else the largest part), shown in magenta: {0}",
        "Zones du nav mesh coupées de la partie principale (là où les joueurs apparaissent, sinon la plus grande), en magenta : {0}",
    ),
    (
        "lab.nav.one_way",
        "One way links: {0}, links to missing areas: {1}",
        "Liens à sens unique : {0}, liens vers des zones absentes : {1}",
    ),
    ("lab.nav.failed", "The nav mesh {0} could not be read: {1}", "Impossible de lire le nav mesh {0} : {1}"),
    (
        "lab.nav.none",
        "No nav mesh found for this map: use Map > Load nav mesh to choose one",
        "Aucun nav mesh trouvé pour cette map : utilisez Map > Charger un nav mesh pour en choisir un",
    ),
    ("lab.nav.files", "Navigation meshes", "Maillages de navigation"),
    ("lab.nav.status", "Nav areas: {0}", "Zones nav : {0}"),
    ("lab.nav.status_isolated", "Nav areas: {0}, cut off: {1}", "Zones nav : {0}, isolées : {1}"),
    ("lab.nav.status_tip", "Navigation mesh shown: {0}", "Nav mesh affiché : {0}"),
    ("lab.nav.reloaded", "Nav mesh reloaded: {0}", "Nav mesh rechargé : {0}"),
    ("lab.bsp.files", "Compiled maps", "Maps compilées"),
    ("lab.bsp.working", "Decompiling {0}...", "Décompilation de {0}..."),
    ("lab.bsp.busy", "A compiled map is already being opened", "Une map compilée est déjà en cours d'ouverture"),
    (
        "lab.bsp.opened",
        "{0} decompiled in {1} ms (brushes: {2}, entities: {3}). Save it as a VMF to keep it.",
        "{0} décompilée en {1} ms (brushes : {2}, entités : {3}). Enregistrez-la en VMF pour la garder.",
    ),
    ("lab.bsp.failed", "{0} could not be decompiled: {1}", "Impossible de décompiler {0} : {1}"),
    ("lab.bsp.failed_brushes", "Brushes that could not be rebuilt: {0}", "Brushes impossibles à reconstruire : {0}"),
    (
        "lab.bsp.empty_brushes",
        "Brushes without faces in the compiled map (it was protected against decompiling): {0}",
        "Brushes sans faces dans la map compilée (elle a été protégée contre la décompilation) : {0}",
    ),
    (
        "lab.bsp.failed_disps",
        "Displacements that could not be rebuilt: {0}",
        "Déplacements impossibles à reconstruire : {0}",
    ),
    (
        "lab.bsp.failed_occluders",
        "Occluders that could not be rebuilt: {0}",
        "Occluders impossibles à reconstruire : {0}",
    ),
    (
        "lab.bsp.lost_overlays",
        "Overlays that lost the faces they were on: {0}",
        "Overlays qui ont perdu les faces sur lesquelles ils étaient : {0}",
    ),
    (
        "lab.bsp.content",
        "Files of the map's own content, shown while it is open: {0}",
        "Fichiers du contenu propre à la map, affichés tant qu'elle est ouverte : {0}",
    ),
    (
        "lab.bsp.limits",
        "A compiled map cannot give everything back: instances are merged, visgroups, groups and hidden objects are gone, and hint, skip and origin brushes were removed by the compiler.",
        "Une map compilée ne rend pas tout : les instances sont fusionnées, les visgroups, groupes et objets cachés ont disparu, et les brushes hint, skip et origin ont été retirés par le compilateur.",
    ),
    ("lab.compare.title", "Compare maps", "Comparer des maps"),
    ("lab.compare.only_here", "Only in this map", "Seulement dans cette map"),
    ("lab.compare.only_there", "Only in the other map", "Seulement dans l'autre map"),
    ("lab.compare.changed", "Changed", "Modifié"),
    ("lab.compare.none", "(none)", "(aucune)"),
    ("lab.compare.key", "{0}: {1} > {2}", "{0} : {1} > {2}"),
    ("lab.compare.class", "class: {0} > {1}", "classe : {0} > {1}"),
    ("lab.compare.outputs", "outputs: {0} > {1}", "sorties : {0} > {1}"),
    ("lab.compare.solids", "brushes: {0} > {1}", "brushes : {0} > {1}"),
    ("lab.compare.geometry", "faces moved", "faces déplacées"),
    ("lab.compare.materials", "material: {0} > {1}", "matériau : {0} > {1}"),
    ("lab.compare.alignment", "texture alignment", "alignement des textures"),
    ("lab.compare.displacement", "displacement", "déplacement"),
    ("lab.compare.editor", "editor data", "données de l'éditeur"),
    (
        "lab.compare.with",
        "Compared with {0} (values read: other map > this map)",
        "Comparée avec {0} (valeurs lues : autre map > cette map)",
    ),
    (
        "lab.compare.same",
        "The two maps hold the same objects with the same content.",
        "Les deux maps contiennent les mêmes objets avec le même contenu.",
    ),
    ("lab.compare.count_here", "Only in this map: {0}", "Seulement dans cette map : {0}"),
    ("lab.compare.count_there", "Only in the other map: {0}", "Seulement dans l'autre map : {0}"),
    ("lab.compare.count_changed", "Changed: {0}", "Modifiés : {0}"),
    ("lab.compare.filter", "Filter by class, id or change...", "Filtrer par classe, id ou changement..."),
    ("lab.compare.select_all", "Select the listed objects", "Sélectionner les objets listés"),
    (
        "lab.compare.select_all_tip",
        "Objects that are only in the other map cannot be selected",
        "Les objets qui ne sont que dans l'autre map ne peuvent pas être sélectionnés",
    ),
    ("lab.compare.entity", "Entity", "Entité"),
    ("lab.compare.solid", "Brush", "Brush"),
    ("lab.compare.more", " (and more: {0})", " (et d'autres : {0})"),
    ("lab.compare.go", "Go to", "Aller à"),
    ("lab.compare.go_tip", "Select it and center the views on it", "Le sélectionner et centrer les vues dessus"),
    ("lab.compare.select", "Select", "Sélectionner"),
    ("lab.compare.working", "Comparing with {0}...", "Comparaison avec {0}..."),
    ("lab.compare.failed", "{0} could not be compared: {1}", "Impossible de comparer avec {0} : {1}"),
    ("lab.compare.files", "Maps", "Maps"),
    (
        "lab.gallery.none",
        "No entity definitions are loaded: the gallery needs the FGD",
        "Aucune définition d'entité chargée : la galerie a besoin du FGD",
    ),
    ("lab.gallery.placed", "Entity gallery: classes placed: {0}", "Galerie d'entités : classes placées : {0}"),
    (
        "lab.particles.label_idle",
        "{0} (not playing: too far or too many)",
        "{0} (à l'arrêt : trop loin ou trop nombreux)",
    ),
    ("lab.particles.label_loading", "{0} (loading...)", "{0} (chargement...)"),
    (
        "lab.particles.label_missing",
        "{0} (not found in the particle files)",
        "{0} (introuvable dans les fichiers de particules)",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean() {
        let problems = hp_app::i18n::lint_table(TR);
        assert!(problems.is_empty(), "{problems:#?}");
        for (key, en, fr) in TR {
            assert!(key.starts_with("lab."), "{key}");
            for i in 0..4 {
                let p = format!("{{{i}}}");
                assert_eq!(en.contains(&p), fr.contains(&p), "{key}: placeholder {p}");
            }
        }
    }
}
