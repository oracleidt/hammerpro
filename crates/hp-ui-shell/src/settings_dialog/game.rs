use egui::{RichText, Ui};
use hp_app::i18n::TrTable;
use hp_app::{AppState, t};

use crate::common::heading;
use crate::game_ui::{
    GameJobs, app_mounts_section, custom_dirs_list, default_is_gmod, game_section, gmod_root_section, mount_toggles,
    path_list, path_row, request,
};
use crate::jobs::PickTarget;

pub(super) fn page(ui: &mut Ui, state: &mut AppState, jobs: &mut GameJobs) {
    let ctx = ui.ctx().clone();
    heading(ui, t!("shell.game.game"));
    game_section(ui, state, jobs);
    let gmod = default_is_gmod(state);
    if gmod {
        heading(ui, t!("shell.game.root"));
        gmod_root_section(ui, state, jobs);

        heading(ui, t!("shell.game.content"));
        mount_toggles(ui, state);
        if ui
            .checkbox(&mut state.settings.game.mount_downloads, t!("shell.game.mount_downloads"))
            .on_hover_text(t!("shell.game.mount_downloads_tip"))
            .changed()
        {
            state.settings_dirty = true;
        }
    }

    heading(ui, t!("shell.game.custom_dirs"));
    custom_dirs_list(ui, state, jobs);

    heading(ui, t!("shell.game.extra_dirs"));
    let (removed, add) = path_list(
        ui,
        "hp_shell_extra_dirs",
        &mut state.settings.game.extra_game_dirs,
        t!("shell.game.extra_dirs_empty"),
    );
    if removed {
        state.settings_dirty = true;
    }
    if add {
        jobs.pick(&ctx, request(PickTarget::ExtraGameDir, state.settings.game.gmod_root.clone()));
    }

    heading(ui, t!("shell.game.apps"));
    app_mounts_section(ui, state);

    heading(ui, t!("shell.game.fgds"));
    let (out, label) = path_row(
        ui,
        t!("shell.game.fgd_override"),
        None,
        &mut state.settings.game.fgd_override,
        if gmod { t!("shell.game.fgd_default_hint") } else { t!("shell.game.fgd_game_hint") },
        !jobs.picking(),
    );
    label.on_hover_text(t!("shell.game.fgd_override_tip"));
    if out.changed {
        state.settings_dirty = true;
    }
    if out.browse {
        let start = state.settings.game.gmod_root.as_ref().map(|r| r.join("bin"));
        jobs.pick(&ctx, request(PickTarget::FgdOverride, start));
    }
    ui.label(RichText::new(t!("shell.game.extra_fgds")).small().strong());
    let (removed, add) =
        path_list(ui, "hp_shell_extra_fgds", &mut state.settings.game.extra_fgds, t!("shell.game.extra_fgds_empty"));
    if removed {
        state.settings_dirty = true;
    }
    if add {
        jobs.pick(&ctx, request(PickTarget::ExtraFgds, None));
    }

    heading(ui, t!("shell.game.maps"));
    let (out, _) = path_row(
        ui,
        t!("shell.game.map_dir"),
        None,
        &mut state.settings.game.map_dir,
        t!("shell.game.map_dir_hint"),
        !jobs.picking(),
    );
    if out.changed {
        state.settings_dirty = true;
    }
    if out.browse {
        jobs.pick(&ctx, request(PickTarget::MapDir, state.settings.game.map_dir.clone()));
    }

    if gmod {
        heading(ui, t!("shell.game.testing"));
        bridge_section(ui, state);
    }
}

fn bridge_section(ui: &mut Ui, state: &mut AppState) {
    let launch = &mut state.runtime.launch;
    let installed = launch.bridge_installed;
    ui.horizontal_wrapped(|ui| {
        let text = match installed {
            Some(true) => t!("shell.game.bridge_installed"),
            Some(false) => t!("shell.game.bridge_missing"),
            None => t!("shell.game.bridge_unknown"),
        };
        ui.label(text).on_hover_text(t!("shell.game.bridge_tip"));
        let busy = launch.bridge_request.is_some();
        if installed != Some(true) {
            if ui
                .add_enabled(!busy && installed.is_some(), egui::Button::new(t!("shell.game.bridge_install")))
                .clicked()
            {
                launch.bridge_request = Some(true);
            }
        } else {
            if ui.add_enabled(!busy, egui::Button::new(t!("shell.game.bridge_update"))).clicked() {
                launch.bridge_request = Some(true);
            }
            if ui.add_enabled(!busy, egui::Button::new(t!("shell.game.bridge_remove"))).clicked() {
                launch.bridge_request = Some(false);
            }
        }
    });
    ui.label(RichText::new(t!("shell.game.bridge_note")).weak().small());
    let s = &mut state.settings.launch;
    if ui
        .add_enabled(
            installed == Some(true),
            egui::Checkbox::new(&mut s.spawn_at_camera, t!("shell.game.spawn_at_camera")),
        )
        .on_hover_text(t!("shell.game.spawn_at_camera_tip"))
        .changed()
    {
        state.settings_dirty = true;
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.game.game", "Game", "Jeu"),
    (
        "shell.game.searching_games",
        "Searching the Steam libraries for games...",
        "Recherche des jeux dans les bibliothèques Steam...",
    ),
    ("shell.game.search_again", "Search again", "Rechercher à nouveau"),
    (
        "shell.game.search_again_tip",
        "Search the Steam libraries again (a game was installed or removed)",
        "Rechercher à nouveau dans les bibliothèques Steam (un jeu a été installé ou désinstallé)",
    ),
    (
        "shell.game.game_note",
        "The game maps are made for: its content is mounted, its entity definitions loaded and maps compile in its format and limits. A map can have its own game (Compile window, Target game).",
        "Le jeu pour lequel les maps sont faites : son contenu est monté, ses définitions d'entités chargées et les maps se compilent dans son format et ses limites. Une map peut avoir son propre jeu (fenêtre Compiler, Jeu cible).",
    ),
    ("shell.game.game_dir", "Game folder", "Dossier du jeu"),
    ("shell.game.game_dir_hint", "Found in Steam when empty", "Trouvé dans Steam si vide"),
    (
        "shell.game.pick_game_dir",
        "Choose the game folder (with gameinfo.txt)",
        "Choisir le dossier du jeu (avec gameinfo.txt)",
    ),
    ("shell.game.game_not_found", "This folder has no gameinfo.txt.", "Ce dossier n'a pas de gameinfo.txt."),
    (
        "shell.game.game_unsupported",
        "Hammer Pro cannot compile maps for the game of this folder ({0}): choose another folder.",
        "Hammer Pro ne peut pas compiler de maps pour le jeu de ce dossier ({0}) : choisissez un autre dossier.",
    ),
    ("shell.game.game_found", "Found: {0}", "Trouvé : {0}"),
    (
        "shell.game.game_missing",
        "Not installed: maps compile in its format with the content of Garry's Mod (choose its folder above if it is installed elsewhere).",
        "Non installé : les maps se compilent dans son format avec le contenu de Garry's Mod (choisissez son dossier ci-dessus s'il est installé ailleurs).",
    ),
    (
        "shell.game.game_format",
        "Format: {0}, BSP version {1}, static props {2}, {3}",
        "Format : {0}, BSP version {1}, props statiques {2}, {3}",
    ),
    ("shell.game.limits_gmod", "the raised limits of Garry's Mod", "les limites relevées de Garry's Mod"),
    ("shell.game.limits_valve", "Valve's limits", "les limites de Valve"),
    ("shell.game.game_fgds", "Entity definitions: {0}", "Définitions d'entités : {0}"),
    (
        "shell.game.game_no_fgd",
        "No entity definitions found for this game: add its FGD below.",
        "Aucune définition d'entités trouvée pour ce jeu : ajoutez son FGD ci-dessous.",
    ),
    ("shell.game.game_exe", "Game: {0}", "Jeu : {0}"),
    (
        "shell.game.game_mapbase",
        "A Mapbase mod: parallax corrected cubemaps (parallax_obb) work in it.",
        "Un mod Mapbase : les cubemaps à correction de parallaxe (parallax_obb) y fonctionnent.",
    ),
    (
        "shell.game.gmod_only_note",
        "Addons, the Workshop, mount.cfg, the Lua skybox switch, testing in game with the bridge addon and Workshop publishing are Garry's Mod features: they are off for this game.",
        "Les addons, le Workshop, mount.cfg, le changement de skybox en Lua, le test en jeu avec l'addon passerelle et la publication sur le Workshop sont propres à Garry's Mod : ils sont désactivés pour ce jeu.",
    ),
    ("shell.game.fgd_game_hint", "The FGD of the game", "Le FGD du jeu"),
    ("shell.game.root", "Garry's Mod folder", "Dossier de Garry's Mod"),
    ("shell.game.root_hint", "Detected automatically when empty", "Détecté automatiquement si vide"),
    (
        "shell.game.auto_detect_note",
        "The folder is detected at startup from Steam. Set it here to use another install.",
        "Le dossier est détecté au démarrage via Steam. Indiquez-le ici pour utiliser une autre installation.",
    ),
    ("shell.game.detect", "Detect", "Détecter"),
    (
        "shell.game.detect_tip",
        "Find Garry's Mod in your Steam libraries",
        "Rechercher Garry's Mod dans vos bibliothèques Steam",
    ),
    ("shell.game.detecting", "Searching Steam libraries...", "Recherche dans les bibliothèques Steam..."),
    ("shell.game.detected", "Garry's Mod found in {0}", "Garry's Mod trouvé dans {0}"),
    (
        "shell.game.not_found",
        "Garry's Mod was not found. Choose its folder with Browse.",
        "Garry's Mod est introuvable. Choisissez son dossier avec Parcourir.",
    ),
    ("shell.game.scanning", "Checking the folder...", "Vérification du dossier..."),
    ("shell.game.rescan", "Check again", "Vérifier à nouveau"),
    ("shell.game.vpk_count", "VPK archives: {0}", "Archives VPK : {0}"),
    ("shell.game.addon_count", "Addons: {0}", "Addons : {0}"),
    (
        "shell.game.invalid_root",
        "This folder does not look like a Garry's Mod install (garrysmod/gameinfo.txt is missing).",
        "Ce dossier ne ressemble pas à une installation de Garry's Mod (garrysmod/gameinfo.txt est absent).",
    ),
    ("shell.game.content", "Content", "Contenu"),
    ("shell.game.mount_addons", "Mount addons", "Monter les addons"),
    (
        "shell.game.mount_addons_tip",
        "Load materials and models from garrysmod/addons",
        "Charger les matériaux et modèles de garrysmod/addons",
    ),
    ("shell.game.mount_workshop", "Mount Workshop downloads", "Monter les téléchargements du Workshop"),
    (
        "shell.game.mount_workshop_tip",
        "Load content of subscribed Workshop addons (slower startup)",
        "Charger le contenu des addons Workshop abonnés (démarrage plus lent)",
    ),
    ("shell.game.mount_cfg", "Use mount.cfg games", "Utiliser les jeux de mount.cfg"),
    (
        "shell.game.mount_cfg_tip",
        "Mount other Source games listed in garrysmod/cfg/mount.cfg",
        "Monter les autres jeux Source listés dans garrysmod/cfg/mount.cfg",
    ),
    (
        "shell.game.mount_downloads",
        "Mount the content downloaded from servers",
        "Monter le contenu téléchargé des serveurs",
    ),
    (
        "shell.game.mount_downloads_tip",
        "Load what servers made the game download (garrysmod/download and garrysmod/cache/workshop). Players may not have it: the release checks pack what the map uses.",
        "Charger ce que les serveurs ont fait télécharger au jeu (garrysmod/download et garrysmod/cache/workshop). Les joueurs ne l'ont pas forcément : les vérifications de publication empaquettent ce que la map utilise.",
    ),
    ("shell.game.custom_dirs", "Custom texture folders", "Dossiers de textures personnalisés"),
    (
        "shell.game.custom_empty",
        "No custom folder. Add folders of VTF/VMT files or plain images (PNG, JPG, TGA, BMP).",
        "Aucun dossier personnalisé. Ajoutez des dossiers de fichiers VTF/VMT ou d'images (PNG, JPG, TGA, BMP).",
    ),
    ("shell.game.add_custom", "Add folder...", "Ajouter un dossier..."),
    (
        "shell.game.custom_tip",
        "Images become materials named prefix/file.",
        "Les images deviennent des matériaux nommés préfixe/fichier.",
    ),
    ("shell.game.prefix", "Prefix", "Préfixe"),
    (
        "shell.game.prefix_tip",
        "Material name prefix, used when the folder is not inside a materials folder",
        "Préfixe des noms de matériaux, utilisé quand le dossier n'est pas dans un dossier materials",
    ),
    ("shell.game.prefix_detected", "detected", "détecté"),
    (
        "shell.game.prefix_detected_tip",
        "Detected from the VMT files of the folder: they reference their textures below this prefix, as in the game. Type another prefix to override it, or clear the field to detect it again.",
        "Détecté d'après les fichiers VMT du dossier : ils désignent leurs textures sous ce préfixe, comme dans le jeu. Saisissez un autre préfixe pour le remplacer, ou videz le champ pour le détecter à nouveau.",
    ),
    (
        "shell.game.prefix_mode_game",
        "The folder has a materials sub folder: material names come from it and the prefix is not used.",
        "Le dossier contient un sous-dossier materials : les noms de matériaux en proviennent et le préfixe n'est pas utilisé.",
    ),
    (
        "shell.game.prefix_mode_inside",
        "The folder is inside a materials folder: material names come from its location and the prefix is not used.",
        "Le dossier se trouve dans un dossier materials : les noms de matériaux dépendent de son emplacement et le préfixe n'est pas utilisé.",
    ),
    ("shell.game.extra_dirs", "Extra game folders", "Dossiers de jeu supplémentaires"),
    ("shell.game.apps", "Steam games by AppID", "Jeux Steam par AppID"),
    (
        "shell.game.apps_empty",
        "No game added: add the AppID of an installed Steam game to use its content (240 Counter-Strike: Source, 440 Team Fortress 2, 220 Half-Life 2...).",
        "Aucun jeu ajouté : ajoutez l'AppID d'un jeu Steam installé pour utiliser son contenu (240 Counter-Strike: Source, 440 Team Fortress 2, 220 Half-Life 2...).",
    ),
    ("shell.game.apps_add", "AppID", "AppID"),
    ("shell.game.apps_installed", "Installed games", "Jeux installés"),
    (
        "shell.game.apps_none_installed",
        "No Source game found in the Steam libraries",
        "Aucun jeu Source trouvé dans les bibliothèques Steam",
    ),
    (
        "shell.game.apps_refresh",
        "Search the Steam libraries again",
        "Rechercher à nouveau dans les bibliothèques Steam",
    ),
    ("shell.game.apps_up", "Higher priority", "Priorité plus haute"),
    ("shell.game.apps_down", "Lower priority", "Priorité plus basse"),
    ("shell.game.apps_missing", "Not installed {0}", "Non installé {0}"),
    ("shell.game.apps_no_game", "No Source game folder in {0}", "Aucun dossier de jeu Source dans {0}"),
    ("shell.game.apps_bad", "\"{0}\" is not a Steam AppID (a number)", "« {0} » n'est pas un AppID Steam (un nombre)"),
    (
        "shell.game.apps_tip",
        "Hammer Pro finds each game in your Steam libraries (libraryfolders.vdf and appmanifest_<AppID>.acf) and mounts what its gameinfo.txt lists, after the content of the map's game, in this order. The browsers, the 3D view and the compile use it.",
        "Hammer Pro trouve chaque jeu dans vos bibliothèques Steam (libraryfolders.vdf et appmanifest_<AppID>.acf) et monte ce que liste son gameinfo.txt, après le contenu du jeu de la map, dans cet ordre. Les navigateurs, la vue 3D et la compilation l'utilisent.",
    ),
    (
        "shell.game.extra_dirs_empty",
        "None. Add mod folders (with a gameinfo.txt or loose content) to mount them too.",
        "Aucun. Ajoutez des dossiers de mod (avec un gameinfo.txt ou du contenu libre) pour les monter aussi.",
    ),
    ("shell.game.fgds", "Entity definitions (FGD)", "Définitions d'entités (FGD)"),
    ("shell.game.fgd_override", "Main FGD", "FGD principal"),
    ("shell.game.fgd_override_tip", "Replaces bin/garrysmod.fgd", "Remplace bin/garrysmod.fgd"),
    ("shell.game.fgd_default_hint", "bin/garrysmod.fgd", "bin/garrysmod.fgd"),
    (
        "shell.game.extra_fgds",
        "Extra FGD files (loaded after the main one)",
        "FGD supplémentaires (chargés après le principal)",
    ),
    ("shell.game.extra_fgds_empty", "No extra FGD", "Aucun FGD supplémentaire"),
    ("shell.game.maps", "Maps", "Maps"),
    ("shell.game.map_dir", "Map folder", "Dossier des maps"),
    ("shell.game.map_dir_hint", "Start folder of Open and Save as", "Dossier proposé par Ouvrir et Enregistrer sous"),
    ("shell.game.pick_root", "Choose the Garry's Mod folder", "Choisir le dossier de Garry's Mod"),
    ("shell.game.pick_extra_dir", "Choose a game folder", "Choisir un dossier de jeu"),
    ("shell.game.pick_map_dir", "Choose the map folder", "Choisir le dossier des maps"),
    ("shell.game.pick_custom_dir", "Choose texture folders", "Choisir des dossiers de textures"),
    ("shell.game.pick_fgd", "Choose FGD files", "Choisir des fichiers FGD"),
    ("shell.game.fgd_filter", "Entity definitions", "Définitions d'entités"),
    ("shell.game.reloading", "Reloading game content...", "Rechargement du contenu du jeu..."),
    ("shell.game.testing", "Testing in game", "Tests en jeu"),
    ("shell.game.bridge_installed", "Hammer Pro bridge: installed", "Pont Hammer Pro : installé"),
    ("shell.game.bridge_missing", "Hammer Pro bridge: not installed", "Pont Hammer Pro : non installé"),
    ("shell.game.bridge_unknown", "Hammer Pro bridge: no game folder", "Pont Hammer Pro : aucun dossier de jeu"),
    (
        "shell.game.bridge_tip",
        "A small addon in garrysmod/addons/hammerpro_bridge that lets Hammer Pro place you at the editor camera and import the props you arrange in game. It is never packed into your maps.",
        "Un petit addon dans garrysmod/addons/hammerpro_bridge qui permet à Hammer Pro de vous placer à la caméra de l'éditeur et d'importer les props que vous disposez en jeu. Il n'est jamais inclus dans vos maps.",
    ),
    ("shell.game.bridge_install", "Install", "Installer"),
    ("shell.game.bridge_update", "Update", "Mettre à jour"),
    ("shell.game.bridge_remove", "Remove", "Supprimer"),
    (
        "shell.game.bridge_note",
        "Restart the game after installing or removing it. It only acts in local games (listen servers).",
        "Redémarrez le jeu après l'avoir installé ou supprimé. Il n'agit que dans les parties locales (serveurs d'écoute).",
    ),
    ("shell.game.spawn_at_camera", "Spawn at the editor camera", "Apparaître à la caméra de l'éditeur"),
    (
        "shell.game.spawn_at_camera_tip",
        "When the map loads in game, place the player where the 3D camera of the editor is (noclip)",
        "Quand la map se charge en jeu, placer le joueur à l'emplacement de la caméra 3D de l'éditeur (noclip)",
    ),
];
