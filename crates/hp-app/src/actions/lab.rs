use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { lab_actions {
    OpenBsp => "open_bsp", File, FileImportOpen, None, Ready;
    OpenBspSniffer => "bsp_sniffer", File, FileImportOpen, None, Ready;
    CompareMaps => "compare_maps", Map, MapInfo, None, Ready;
    EntityGallery => "entity_gallery", Map, MapProperties, None, Ready;
    ShowLogicalView => "show_logical_view", View, ViewType, None, Ready;
    ToggleNavMesh => "toggle_nav_mesh", View, ViewDiagnostics, None, Ready;
    PhysicsPlacement => "physics_placement", Tools, ToolsScatter, None, Ready;
    ToolPhysics => "tool_physics", Tools, ToolsSwitch, None, Ready;
    BrushesToProp => "brushes_to_prop", Tools, ToolsProps, None, Ready;
    PropToBrushes => "prop_to_brushes", Tools, ToolsProps, None, Ready;
    GenerateNavMesh => "generate_nav_mesh", Tools, ToolsPaths, None, Ready;
    LoadNavMesh => "load_nav_mesh", Map, MapNavMesh, None, Ready;
    ToggleParticles => "toggle_particles", View, ViewEffectsObjects, None, Ready;
    ToggleParticlesSelected => "toggle_particles_selected", View, ViewEffectsObjects, None, Ready;
    ToggleEntityFx => "toggle_entity_fx", View, ViewEffects, None, Ready;
    ParticlePreview => "particle_preview", View, ViewPanels, None, Ready;
    ParticleEditor => "particle_editor", Tools, ToolsContent, None, Ready;
    FlyApplyTexture => "fly_apply_texture", Tools, Hidden, sc(NONE, Key::T), Ready;
    FlyApplyTextureBrush => "fly_apply_texture_brush", Tools, Hidden, sc(NONE, Key::F), Ready;
    FlyPickTexture => "fly_pick_texture", Tools, Hidden, sc(NONE, Key::V), Ready;
    FlyAlignTexture => "fly_align_texture", Tools, Hidden, sc(NONE, Key::N), Ready;
    ImportModel => "import_model", File, FileImport, None, Ready;
    ImportModelsBatch => "import_models_batch", File, FileImport, None, Ready;
    SendToBlender => "send_to_blender", Tools, ToolsBlender, None, Ready;
    PullFromBlender => "pull_from_blender", Tools, ToolsBlender, None, Ready;
    StopBlenderLink => "stop_blender_link", Tools, ToolsBlender, None, Ready;
    InstallBlenderAddon => "install_blender_addon", Tools, ToolsBlender, None, Ready;
    TogglePlayerView => "toggle_player_view", View, ViewPlayer, sc(NONE, Key::F6), Ready;
    OpenSoundEditor => "sound_editor", Tools, ToolsContent, None, Ready;
    OpenColorCorrectionEditor => "color_correction_editor", Tools, ToolsContent, None, Ready;
    ToggleColorCorrectionPreview => "toggle_color_correction_preview", View, ViewEnvironment, None, Ready;
    PropperOptions => "propper_options", Tools, ToolsProps, None, Ready;
    CombineProps => "combine_props", Tools, ToolsProps, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::ToggleParticlesSelected => Some(state.settings.lab.show_particles),
        Action::FlyApplyTexture | Action::FlyApplyTextureBrush | Action::FlyPickTexture | Action::FlyAlignTexture => {
            Some(state.runtime.lab.fly_texture.active)
        }
        Action::PropToBrushes => Some(selected_prop(state).is_some()),
        Action::CombineProps => Some(selected_static_props(state).len() >= 2 && !state.runtime.lab.combine_busy),
        Action::SendToBlender => Some(!state.doc.selection().is_empty()),
        Action::PullFromBlender | Action::StopBlenderLink => Some(state.runtime.blender.link.is_some()),
        Action::BrushesToProp => {
            Some(state.doc.selection().objects.iter().any(|o| matches!(o, hp_editor::ObjRef::Solid(_))))
        }
        _ => None,
    }
}

pub fn selected_prop(state: &AppState) -> Option<hp_editor::Id> {
    let scene = state.doc.scene();
    state.doc.selection().objects.iter().find_map(|o| match *o {
        hp_editor::ObjRef::Entity(id) => scene.entity(id).filter(|n| is_model_prop(&n.entity)).map(|_| id),
        hp_editor::ObjRef::Solid(_) => None,
    })
}

pub fn selected_static_props(state: &AppState) -> Vec<hp_editor::Id> {
    let scene = state.doc.scene();
    state
        .doc
        .selection()
        .objects
        .iter()
        .filter_map(|o| match *o {
            hp_editor::ObjRef::Entity(id) => scene
                .entity(id)
                .filter(|n| is_model_prop(&n.entity) && n.entity.classname.eq_ignore_ascii_case("prop_static"))
                .map(|_| id),
            hp_editor::ObjRef::Solid(_) => None,
        })
        .collect()
}

pub fn is_model_prop(e: &hp_vmf::Entity) -> bool {
    e.classname.to_ascii_lowercase().starts_with("prop_")
        && e.get("model").is_some_and(|m| m.trim().to_ascii_lowercase().ends_with(".mdl"))
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::ShowLogicalView => Some(Check::Toggle(state.runtime.lab.logic_open)),
        Action::PropperOptions => Some(Check::Toggle(state.runtime.lab.propper_options_open)),
        Action::CombineProps => Some(Check::Toggle(state.runtime.lab.combine_open)),
        Action::OpenBspSniffer => Some(Check::Toggle(state.runtime.lab.sniffer.open)),
        Action::OpenSoundEditor => Some(Check::Toggle(state.runtime.lab.sound_color.sound_open)),
        Action::OpenColorCorrectionEditor => Some(Check::Toggle(state.runtime.lab.sound_color.color_open)),
        Action::ToggleColorCorrectionPreview => Some(Check::Toggle(state.settings.lab.color_correction_preview)),
        Action::ToggleNavMesh => Some(Check::Toggle(state.settings.lab.show_nav_mesh)),
        Action::ToggleParticles => Some(Check::Toggle(state.settings.lab.show_particles)),
        Action::ToggleParticlesSelected => Some(Check::Toggle(state.settings.lab.particles_selected_only)),
        Action::ToggleEntityFx => Some(Check::Toggle(state.settings.lab.show_entity_fx)),
        Action::ParticlePreview => Some(Check::Toggle(state.runtime.lab.particle_preview.open)),
        Action::TogglePlayerView => Some(Check::Toggle(state.runtime.lab.player.active)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::OpenBsp => Some("📦"),
        Action::ShowLogicalView => Some("🔀"),
        Action::CompareMaps => Some("⚖"),
        Action::ToggleNavMesh => Some("🗺"),
        Action::TogglePlayerView => Some("🚶"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.tool_physics", "Physics simulator", "Simulateur physique"),
    ("tool.physics", "Physics", "Physique"),
    ("shell.undo.physics", "Simulate physics", "Simulation physique"),
    ("action.open_bsp", "BSP Decompiler...", "Décompilateur de BSP..."),
    ("action.bsp_sniffer", "BSP Sniffer...", "BSP Sniffer..."),
    ("action.import_model", "Import a model (FBX, OBJ, glTF)...", "Importer un modèle (FBX, OBJ, glTF)..."),
    ("action.import_models_batch", "Import models (batch)...", "Importer des modèles (lot)..."),
    ("action.send_to_blender", "Send to Blender", "Envoyer vers Blender"),
    ("action.pull_from_blender", "Update from Blender", "Mettre à jour depuis Blender"),
    ("action.stop_blender_link", "Stop the Blender live link", "Arrêter le lien direct avec Blender"),
    ("action.install_blender_addon", "Install the Blender add-on", "Installer l'add-on Blender"),
    ("shell.undo.update_from_blender", "Update from Blender", "Mise à jour depuis Blender"),
    ("shell.undo.import_models", "Import models", "Import de modèles"),
    ("shell.undo.combine_props", "Combine props", "Combinaison de props"),
    ("action.compare_maps", "Compare with another map...", "Comparer avec une autre map..."),
    ("action.entity_gallery", "Insert entity gallery", "Insérer la galerie d'entités"),
    ("action.show_logical_view", "Logical view", "Vue logique"),
    ("action.toggle_nav_mesh", "Show nav mesh", "Afficher le nav mesh"),
    ("action.physics_placement", "Physics placement (drop and settle)", "Placement physique (chute et pose)"),
    (
        "lab.physplace.nothing",
        "Nothing to drop: the selection is already resting on a surface.",
        "Rien à faire tomber : la sélection repose déjà sur une surface.",
    ),
    ("shell.undo.physics_placement", "Physics placement", "Placement physique"),
    ("shell.undo.push_props", "Push props", "Pousser des props"),
    ("action.brushes_to_prop", "Convert brushes to a prop...", "Convertir les brushes en prop..."),
    ("lab.lod.enabled", "Levels of detail (LOD)", "Niveaux de détail (LOD)"),
    (
        "lab.lod.enabled_tip",
        "Simplified versions of the model that the game draws far away (fewer triangles, same look).",
        "Versions simplifiées du modèle que le jeu affiche de loin (moins de triangles, même apparence).",
    ),
    ("lab.lod.levels", "Levels", "Niveaux"),
    (
        "lab.lod.levels_tip",
        "Levels below the full model (3 at least; big models get one or two more).",
        "Niveaux sous le modèle complet (3 au moins ; les gros modèles en ont un ou deux de plus).",
    ),
    ("lab.lod.reduction", "Each keeps", "Chacun garde"),
    ("lab.lod.automatic", "Automatic distance", "Distance automatique"),
    (
        "lab.lod.metric_tip",
        "Source LOD switch value ($lod). Larger values keep the previous level longer. The actual camera distance also depends on FOV and screen resolution.",
        "Seuil de bascule Source ($lod). Une valeur plus élevée conserve le niveau précédent plus longtemps. La distance réelle dépend aussi du FOV et de la résolution.",
    ),
    (
        "lab.lod.distances_tip",
        "Set each LOD switch distance. Large models may use extra levels. Distances are kept in increasing order when exported.",
        "Réglez la distance de chaque LOD. Les gros modèles peuvent utiliser des niveaux supplémentaires. Les distances sont exportées dans l'ordre croissant.",
    ),
    (
        "lab.lod.reduction_tip",
        "Part of the triangles of the level above each level keeps (small models are reduced more gently).",
        "Part des triangles du niveau précédent que garde chaque niveau (les petits modèles sont réduits plus doucement).",
    ),
    ("action.propper_options", "Prop options (Propper)...", "Options des props (Propper)..."),
    ("action.combine_props", "Combine props...", "Combiner des props..."),
    (
        "lab.prop.sandboxed",
        "Automated runs do not compile models (studiomdl writes into the game).",
        "Les lancements automatisés ne compilent pas de modèles (studiomdl écrit dans le jeu).",
    ),
    ("lab.prop.select", "Select the brushes to convert into a prop.", "Sélectionnez les brushes à convertir en prop."),
    (
        "lab.prop.no_game",
        "Set the game folder first (Settings > Game).",
        "Configurez d'abord le dossier du jeu (Paramètres > Jeu).",
    ),
    (
        "lab.prop.no_studiomdl",
        "studiomdl was not found in the game's bin folder.",
        "studiomdl est introuvable dans le dossier bin du jeu.",
    ),
    (
        "lab.prop.empty",
        "The selection has no visible faces to make a prop from.",
        "La sélection n'a aucune face visible pour faire un prop.",
    ),
    (
        "lab.prop.failed",
        "Could not write the model source files.",
        "Impossible d'écrire les fichiers source du modèle.",
    ),
    (
        "lab.prop.preparing",
        "Preparing a {0} triangle prop (blend textures, levels of detail, collision): {1}",
        "Préparation d'un prop de {0} triangles (textures de mélange, niveaux de détail, collision) : {1}",
    ),
    ("lab.prop.compiling", "Compiling a {0} triangle prop: {1}", "Compilation d'un prop de {0} triangles : {1}"),
    (
        "lab.prop.compile_failed",
        "studiomdl could not compile the prop (see the console).",
        "studiomdl n'a pas pu compiler le prop (voir la console).",
    ),
    ("lab.prop.done", "Brushes converted to {0}.", "Brushes convertis en {0}."),
    (
        "lab.prop.replace_failed",
        "The model compiled but the brushes could not be replaced: {0}",
        "Le modèle a compilé mais les brushes n'ont pas pu être remplacés : {0}",
    ),
    ("shell.undo.convert_to_static_prop", "Convert to static prop", "Conversion en prop statique"),
    ("shell.undo.convert_to_dynamic_prop", "Convert to dynamic prop", "Conversion en prop dynamique"),
    ("shell.undo.convert_to_physics_prop", "Convert to physics prop", "Conversion en prop physique"),
    ("action.prop_to_brushes", "Convert prop to brushes...", "Convertir le prop en brushes..."),
    ("shell.undo.convert_prop_to_brushes", "Convert prop to brushes", "Conversion du prop en brushes"),
    ("action.generate_nav_mesh", "Generate nav mesh", "Générer le nav mesh"),
    (
        "lab.navgen.save_first",
        "Save the map first: the nav mesh is written next to it.",
        "Enregistrez d'abord la map : le nav mesh est écrit à côté.",
    ),
    (
        "lab.navgen.no_spawn",
        "No player start in the map: add an info_player_start where players walk.",
        "Aucun point d'apparition dans la map : ajoutez un info_player_start là où les joueurs marchent.",
    ),
    ("lab.navgen.working", "Generating the nav mesh...", "Génération du nav mesh..."),
    (
        "lab.navgen.nothing",
        "no walkable ground found around the player starts",
        "aucun sol praticable trouvé autour des points d'apparition",
    ),
    ("lab.navgen.crashed", "the generator stopped", "le générateur s'est arrêté"),
    ("lab.navgen.done", "Nav mesh generated: {0} areas in {1} ms", "Nav mesh généré : {0} zones en {1} ms"),
    (
        "lab.navgen.capped",
        "The map is very large: the nav mesh stopped at its sample limit.",
        "La map est très grande : le nav mesh s'est arrêté à sa limite d'échantillons.",
    ),
    (
        "lab.navgen.bad_seeds",
        "{0} player start(s) have no ground under them and were skipped.",
        "{0} point(s) d'apparition sans sol en dessous ont été ignorés.",
    ),
    ("lab.navgen.failed", "Nav mesh generation failed: {0}", "Échec de la génération du nav mesh : {0}"),
    ("action.load_nav_mesh", "Load nav mesh...", "Charger un nav mesh..."),
    ("action.toggle_player_view", "Player view", "Vue joueur"),
    ("action.sound_editor", "Sound editor...", "Éditeur de sons..."),
    ("action.color_correction_editor", "Color correction editor...", "Éditeur de correction des couleurs..."),
    (
        "action.toggle_color_correction_preview",
        "Preview color_correction entities",
        "Prévisualiser les entités color_correction",
    ),
    ("action.toggle_particles", "Show particles", "Afficher les particules"),
    ("action.toggle_particles_selected", "Particles of the selection only", "Particules de la sélection uniquement"),
    ("action.toggle_entity_fx", "Preview volume effects", "Aperçu des effets de volume"),
    ("action.particle_preview", "Particle preview...", "Aperçu des particules..."),
    ("action.particle_editor", "Particle editor...", "Éditeur de particules..."),
    (
        "action.fly_apply_texture",
        "Camera mode: apply the texture to the targeted face",
        "Mode caméra : appliquer la texture à la face visée",
    ),
    (
        "action.fly_apply_texture_brush",
        "Camera mode: apply the texture to the whole targeted brush",
        "Mode caméra : appliquer la texture à tout le brush visé",
    ),
    (
        "action.fly_pick_texture",
        "Camera mode: pick the texture of the targeted face",
        "Mode caméra : prélever la texture de la face visée",
    ),
    (
        "action.fly_align_texture",
        "Camera mode: align the targeted face texture to the view",
        "Mode caméra : aligner la texture de la face visée sur la vue",
    ),
    ("shell.undo.move_in_logical_view", "Move in logical view", "Déplacement dans la vue logique"),
    ("shell.undo.arrange_logical_view", "Arrange logical view", "Organisation de la vue logique"),
    ("shell.undo.insert_entity_gallery", "Insert entity gallery", "Insertion de la galerie d'entités"),
];
