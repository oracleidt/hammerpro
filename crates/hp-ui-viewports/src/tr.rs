use hp_app::i18n::TrTable;

pub const TR: TrTable = &[
    ("viewport.camera_speed", "Camera speed: {0} units/s", "Vitesse de la caméra : {0} unités/s"),
    ("viewport.starting", "Starting renderer...", "Démarrage du rendu..."),
    (
        "viewport.collision_legend",
        "Collision models only: green, what collides; orange box, a model without a collision model",
        "Modèles de collision seuls : en vert, ce qui collisionne ; boîte orange, un modèle sans modèle de collision",
    ),
    ("viewport.change_view", "Change the view shown here", "Changer la vue affichée ici"),
    ("viewport.view_type", "View", "Vue"),
    (
        "viewport.logical_none",
        "No schematic is registered in this build",
        "Aucun schéma n'est enregistré dans cette version",
    ),
    ("viewport.disp_2d", "Displacements", "Displacements"),
    ("viewport.disp_global", "Like the other views ({0})", "Comme les autres vues ({0})"),
    ("viewport.disp_mesh", "Full mesh", "Maillage complet"),
    ("viewport.disp_selected", "Mesh of the selection", "Maillage de la sélection"),
    ("viewport.disp_outline", "Outline only", "Contour seulement"),
    ("viewport.disp_source", "Source brushes", "Brushes d'origine"),
    (
        "viewport.disp_source_tip",
        "Draw the brush each displacement was built from as a faint outline (every 2D view)",
        "Dessine en contour discret le brush à partir duquel chaque displacement a été créé (toutes les vues 2D)",
    ),
    ("viewport.walk_title", "Displacement walkability", "Praticabilité des displacements"),
    ("viewport.walk_buildable", "Walkable, buildable", "Praticable, constructible"),
    ("viewport.walk_walkable", "Walkable", "Praticable"),
    ("viewport.walk_steep", "Not walkable", "Non praticable"),
    ("viewport.walk_below", "Up to {0} degrees", "Jusqu'à {0} degrés"),
    ("viewport.walk_above", "Above {0} degrees", "Au-dessus de {0} degrés"),
    ("viewport.walk_band", "{0} to {1} degrees", "{0} à {1} degrés"),
    ("viewport.walk_overhang", "Overhang", "Surplomb"),
    ("viewport.restore", "Restore the four views", "Restaurer les quatre vues"),
    ("viewport.zoom", "{0}%", "{0} %"),
    ("viewport.camera_pos", "Camera: {0}", "Caméra : {0}"),
    ("viewport.camera_angles", "Yaw {0}°, pitch {1}°", "Lacet {0}°, tangage {1}°"),
    ("viewport.center_here", "Center views here", "Centrer les vues ici"),
    ("viewport.create_here", "Create {0} here", "Créer {0} ici"),
    ("viewport.create_entity_here", "Create entity here", "Créer une entité ici"),
    ("viewport.team_ping", "Ping here for the team", "Signaler ici à l'équipe"),
    ("viewport.drop_apply_face", "Apply {0} to this face", "Appliquer {0} à cette face"),
    ("viewport.drop_apply_selection", "Apply {0} to the selection", "Appliquer {0} à la sélection"),
    ("viewport.drop_no_face", "Drop the material on a face", "Déposez le matériau sur une face"),
    (
        "viewport.drop_no_selection",
        "Select objects first to apply a material from a 2D view",
        "Sélectionnez d'abord des objets pour appliquer un matériau depuis une vue 2D",
    ),
    ("viewport.drop_place", "Place {0}", "Placer {0}"),
    ("viewport.drop_paste_prefab", "Paste prefab {0}", "Coller le prefab {0}"),
    (
        "viewport.brush_class",
        "{0} is a brush entity: select brushes and use Tie to entity",
        "{0} est une entité de brush : sélectionnez des brushes puis utilisez Lier à une entité",
    ),
    ("viewport.edit_failed", "Could not complete the operation: {0}", "Impossible de terminer l'opération : {0}"),
    ("viewport.prefab_read_failed", "Could not read the prefab {0}: {1}", "Impossible de lire le prefab {0} : {1}"),
    ("viewport.material_applied", "Applied {0} to {1} face(s)", "{0} appliqué à {1} face(s)"),
    (
        "viewport.model_unavailable",
        "Model information unavailable for {0}: {1}",
        "Informations du modèle indisponibles pour {0} : {1}",
    ),
    (
        "viewport.pick_entity",
        "Click an entity to pick it (Esc to cancel)",
        "Cliquez sur une entité pour la choisir (Échap pour annuler)",
    ),
    (
        "viewport.pick_faces",
        "Click a face in the 3D view, Ctrl+click to add more, Enter to finish ({0} added, Esc to cancel)",
        "Cliquez sur une face dans la vue 3D, Ctrl+clic pour en ajouter, Entrée pour terminer ({0} ajoutées, Échap pour annuler)",
    ),
    (
        "viewport.pick_point",
        "Click a point in a view (Esc to cancel)",
        "Cliquez sur un point dans une vue (Échap pour annuler)",
    ),
    (
        "viewport.pick_angles",
        "Click where the entity should point (Esc to cancel)",
        "Cliquez à l'endroit vers lequel l'entité doit pointer (Échap pour annuler)",
    ),
    ("viewport.player.title", "Player view", "Vue joueur"),
    ("viewport.player.mode_noclip", "noclip", "noclip"),
    ("viewport.player.mode_third", "third person", "3e personne"),
    ("viewport.player.mode_first", "first person", "1re personne"),
    ("viewport.player.head", "{0} - {1} - {2} fps - {3}", "{0} - {1} - {2} ips - {3}"),
    ("viewport.player.state", "speed {0} - feet {1}", "vitesse {0} - pieds {1}"),
    ("viewport.player.wireframe", "mat_wireframe {0}: {1}", "mat_wireframe {0} : {1}"),
    ("viewport.player.wire0", "off", "désactivé"),
    (
        "viewport.player.wire1",
        "world triangles seen through, models in wireframe",
        "triangles du monde vus à travers, modèles en filaire",
    ),
    (
        "viewport.player.wire2",
        "world triangles on the visible surfaces, models in wireframe",
        "triangles du monde sur les surfaces visibles, modèles en filaire",
    ),
    (
        "viewport.player.wire3",
        "world polygon outlines seen through, models in wireframe",
        "contours des polygones du monde vus à travers, modèles en filaire",
    ),
    (
        "viewport.player.counts_all",
        "Drawn: {0} draw calls, {1} triangles, {2} material changes",
        "Dessiné : {0} appels, {1} triangles, {2} changements de matériau",
    ),
    (
        "viewport.player.counts_world",
        "World: {0} draw calls, {1} triangles, {2} chunks in view, {3} occluded",
        "Monde : {0} appels, {1} triangles, {2} blocs visibles, {3} masqués",
    ),
    (
        "viewport.player.counts_models",
        "Models: {0} draw calls, {1} triangles, {2} instances, {3} sprites",
        "Modèles : {0} appels, {1} triangles, {2} instances, {3} sprites",
    ),
    (
        "viewport.player.game_cluster",
        "In game: cluster {0}, area {3}, draws {1} of {2} clusters (PVS and area portals)",
        "En jeu : cluster {0}, zone {3}, dessine {1} clusters sur {2} (PVS et area portals)",
    ),
    (
        "viewport.player.game_counts",
        "In game: {0} faces, {1} triangles, {2} displacements, {3} props, {4} overlays",
        "En jeu : {0} faces, {1} triangles, {2} déplacements, {3} props, {4} overlays",
    ),
    (
        "viewport.player.no_vis",
        "In game: no visibility data (compile the map with VVIS)",
        "En jeu : pas de données de visibilité (compilez la map avec VVIS)",
    ),
    (
        "viewport.player.keys",
        "X wireframe - C camera - V noclip - M model - N nodraw - B compiled world - L lighting - K hearing - wheel distance - E use - T trains - H hide - Esc quit",
        "X filaire - C caméra - V noclip - M modèle - N nodraw - B monde compilé - L éclairage - K écoute - molette distance - E utiliser - T trains - H masquer - Échap quitter",
    ),
    (
        "viewport.player.hearing_off",
        "Hearing: off (K plays the soundscapes and ambient_generic sounds)",
        "Écoute : coupée (K joue les soundscapes et les sons ambient_generic)",
    ),
    (
        "viewport.player.hearing_on",
        "Hearing: soundscape {0} ({1} looping sounds, {2} ambient_generic), volume {3}",
        "Écoute : soundscape {0} ({1} sons en boucle, {2} ambient_generic), volume {3}",
    ),
    (
        "viewport.player.hearing_none",
        "Hearing: no soundscape in range yet ({0} ambient_generic), volume {1}",
        "Écoute : aucun soundscape à portée pour l'instant ({0} ambient_generic), volume {1}",
    ),
    (
        "viewport.player.light_on",
        "Lighting: the lightmaps of the compiled map ({0}), like in game",
        "Éclairage : les lightmaps de la map compilée ({0}), comme en jeu",
    ),
    (
        "viewport.player.light_none",
        "Lighting: none in the compiled map (no VRAD): fullbright, like in game",
        "Éclairage : aucun dans la map compilée (pas de VRAD) : plein éclairage, comme en jeu",
    ),
    ("viewport.player.light_and_cubemaps", "{0}; {1}", "{0} ; {1}"),
    ("viewport.player.cubemaps_off", "reflections off", "reflets désactivés"),
    ("viewport.player.cubemaps_none", "cubemaps: none in the map (the sky)", "cubemaps : aucune dans la map (le ciel)"),
    ("viewport.player.cubemaps_built", "cubemaps: built in game ({0})", "cubemaps : construites en jeu ({0})"),
    (
        "viewport.player.cubemaps_drawn",
        "cubemaps: drawn by Hammer Pro ({0}, the map has VBSP's placeholders only)",
        "cubemaps : dessinées par Hammer Pro ({0}, la map n'a que les cubemaps provisoires de VBSP)",
    ),
    (
        "viewport.player.cubemaps_both",
        "cubemaps: {0} built in game, {1} drawn by Hammer Pro",
        "cubemaps : {0} construites en jeu, {1} dessinées par Hammer Pro",
    ),
    (
        "viewport.player.light_off",
        "Lighting: editor shading (L: the lightmaps of the compiled map)",
        "Éclairage : ombrage de l'éditeur (L : les lightmaps de la map compilée)",
    ),
    (
        "viewport.player.bsp_counts",
        "Compiled world in view: {0} faces, {1} triangles, {2} displacements, {3} materials, {4} back faces skipped",
        "Monde compilé dans la vue : {0} faces, {1} triangles, {2} déplacements, {3} matériaux, {4} faces de dos ignorées",
    ),
    (
        "viewport.player.bsp_no_portals",
        "Without area portals: {0} faces, {1} triangles, {2} displacements",
        "Sans area portals : {0} faces, {1} triangles, {2} déplacements",
    ),
    (
        "viewport.player.game_areas",
        "Area portals: {0} of {1} open, {2} looked through - areas drawn: {3} of {4}",
        "Area portals : {0} ouverts sur {1}, {2} traversés - zones dessinées : {3} sur {4}",
    ),
    (
        "viewport.player.bsp_stale",
        "The map changed since it was compiled: compile it again to see the changes here",
        "La map a changé depuis la compilation : recompilez pour voir les changements ici",
    ),
    (
        "viewport.player.bsp_editor",
        "Editor brushes shown (B: compiled world)",
        "Brushes de l'éditeur affichés (B : monde compilé)",
    ),
    (
        "viewport.player.bsp_none",
        "Editor brushes: compile the map to see the world as the game cuts it",
        "Brushes de l'éditeur : compilez la map pour voir le monde découpé comme en jeu",
    ),
    ("viewport.player.hidden", "H: show the counters", "H : afficher les compteurs"),
    ("viewport.player.io_title", "Inputs and outputs:", "Entrées et sorties :"),
    (
        "viewport.player.styles",
        "Light styles: {0} in use, animated for {1} s",
        "Styles de lumière : {0} utilisés, animés depuis {1} s",
    ),
    ("viewport.player.style", "Style {0}: {1} ({2})", "Style {0} : {1} ({2})"),
    ("viewport.player.styles_more", "and {0} more", "et {0} de plus"),
    ("viewport.player.style_light", "[{0}] {1} (style {2}): {3}, {4} ({5})", "[{0}] {1} (style {2}) : {3}, {4} ({5})"),
    ("viewport.player.light_switch_on", "on", "allumée"),
    ("viewport.player.light_switch_off", "off", "éteinte"),
    (
        "viewport.player.style_keys",
        "1 to 9: turn a named light on or off",
        "1 à 9 : allumer ou éteindre une lumière nommée",
    ),
    (
        "viewport.player.trains",
        "Trains: {0} moving of {1} - T: start or stop them all, E on a train: start or stop it",
        "Trains : {0} en marche sur {1} - T : les démarrer ou les arrêter tous, E sur un train : le démarrer ou l'arrêter",
    ),
    (
        "viewport.io.train_no_control",
        "{0}: no user control (spawnflag 2), use T or its inputs",
        "{0} : pas de contrôle par le joueur (spawnflag 2), utilisez T ou ses entrées",
    ),
    ("viewport.io.no_target", "-> {0}.{1}: no entity of that name", "-> {0}.{1} : aucune entité de ce nom"),
    ("viewport.io.not_simulated", "(not simulated)", "(non simulé)"),
    (
        "viewport.player.started",
        "Player view: Esc or F6 to go back to the editor.",
        "Vue joueur : Échap ou F6 pour revenir à l'éditeur.",
    ),
    (
        "viewport.player.no_3d_view",
        "The player view needs a 3D view in the layout.",
        "La vue joueur a besoin d'une vue 3D dans la disposition.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_clean() {
        let problems = hp_app::i18n::lint_table(TR);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn no_collision_with_core_table() {
        for (key, _, _) in TR {
            assert!(key.starts_with("viewport."), "key {key} must use the viewport. prefix");
            assert!(!hp_app::strings::TR.iter().any(|(k, _, _)| k == key), "key {key} collides with the core table");
        }
    }

    #[test]
    fn placeholders_match_between_languages() {
        for (key, en, fr) in TR {
            for i in 0..4 {
                let p = format!("{{{i}}}");
                assert_eq!(en.contains(&p), fr.contains(&p), "{key}: placeholder {p} differs");
            }
        }
    }
}
