[Documentation](../README.md) > Référence > Paramètres | [English](../../en/reference/settings.md) | [Русский](../../ru/reference/settings.md)

# Référence des paramètres

Toutes les options de Hammer Pro 1.0.2 sont rangées dans un seul fichier TOML, `settings.toml`, dans `%APPDATA%\HammerPro` sous Windows (le dossier de configuration de l'utilisateur sur les autres systèmes). Cette page liste chaque table et chaque clé de ce fichier, sa valeur par défaut, son effet et l'endroit de l'interface où on la règle.

En temps normal, vous n'ouvrez jamais ce fichier : la [fenêtre des paramètres](../features/settings-dialog.md) (**Outils > Options...**, `Ctrl+,`) et les menus le modifient, et chaque changement est enregistré tout de suite. Le modifier à la main reste sans danger :

- Chaque clé a une valeur par défaut. Une clé absente prend sa valeur par défaut, une clé inconnue est ignorée : une ancienne et une nouvelle version de Hammer Pro peuvent partager le même fichier.
- Une table n'est écrite que si l'une de ses valeurs diffère des valeurs par défaut : un fichier neuf est court.
- Si le fichier est illisible (une faute de syntaxe TOML), Hammer Pro démarre avec les valeurs par défaut et écrit un avertissement dans le journal. Fermez Hammer Pro avant de modifier le fichier, sinon le prochain enregistrement écrase votre modification.
- Le fichier est écrit de façon atomique (un fichier temporaire, puis un renommage).
- Les outils MCP `get_settings` et `set_setting` lisent et modifient ces clés par leur chemin pointé (`render.water_preview`, `edit.grid_size`).

Les valeurs suivent la syntaxe TOML : `true`, `16.0`, `"Dark"`. Les énumérations s'écrivent avec le nom de variante indiqué dans les tableaux (en anglais, c'est le format du fichier).

## Clés de premier niveau

| Clé | Défaut | Effet |
| --- | --- | --- |
| `language` | langue du système | `"En"`, `"Fr"` ou `"Ru"`. Langue de l'interface. Au premier lancement, elle vient de la langue du système. **Paramètres > Général**, ou l'argument `--lang` pour un seul lancement. |
| `language_chosen` | `false` | Passe à `true` une fois la langue choisie la première fois. |
| `recent_files` | `[]` | Les maps de **Fichier > Maps récentes**, la plus récente en premier, 12 au plus. **Paramètres > Général** permet de vider la liste. |
| `keybindings` | `{}` | Vos raccourcis modifiés : id de l'action = texte du raccourci (`undo = "Ctrl+Alt+Z"`, `""` pour aucun raccourci). Les noms de touches sont en anglais (`Shift`, pas `Maj`). Seules les actions modifiées sont écrites. Les ids sont listés dans [Actions et raccourcis clavier](actions-shortcuts.md). **Paramètres > Clavier**. |

## `[view]` : vues et caméra

Surtout sur **Paramètres > Vues** ; les clés `show_*` sont aussi dans le menu **Affichage**.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `render_mode` | `"Textured"` | Mode de rendu 3D : `Textured`, `Flat`, `Wireframe`, `TexturedWire`, `Lit`, `Unshaded`, `SmoothingGroups`, `Baked`, `LightmapGrid`. **Affichage > Mode de rendu 3D**. |
| `fov_deg` | `75.0` | Champ de vision de la vue 3D, en degrés. |
| `camera_speed` | `1000.0` | Vitesse de vol de la caméra 3D, en unités par seconde. |
| `camera_fast_multiplier` | `3.0` | Multiplicateur de vitesse quand `Maj` est enfoncée. |
| `mouse_sensitivity` | `0.15` | Sensibilité de la souris pour regarder autour. |
| `invert_mouse_y` | `false` | Inverse l'axe vertical de la souris. |
| `far_plane` | `32768.0` | Distance d'affichage de la vue 3D. **Affichage > Zoom et distance d'affichage > Distance d'affichage plus courte / plus longue** la modifient aussi. |
| `msaa` | `true` | Anticrénelage des vues. |
| `show_models` | `true` | Affiche les props avec leurs modèles. **Affichage > Options d'affichage > Afficher les modèles**. |
| `show_helpers` | `true` | Affiche les aides des entités et les sprites. **Affichage > Options d'affichage > Afficher les aides et sprites**. |
| `show_tool_textures` | `true` | Affiche les faces en textures outils. **Affichage > Options d'affichage > Afficher les textures outils**. |
| `show_fps` | `false` | Affiche les images par seconde. **Paramètres > Général**. |
| `show_3d_grid` | `false` | Grille dans la vue 3D (page Vues). |
| `grid_highlight_every` | `8` | Une ligne de grille 2D sur N est mise en valeur. |
| `show_1024_grid` | `true` | Lignes toutes les 1024 unités dans les vues 2D. |
| `background_2d` | `[20, 20, 22]` | Couleur de fond des vues 2D (RVB). |
| `background_3d` | `[36, 38, 44]` | Couleur de fond de la vue 3D (RVB). |
| `selection_color` | `[255, 70, 70]` | Couleur de la sélection (RVB). |
| `zoom_to_cursor` | `true` | La molette zoome vers le curseur (désactivé : vers le centre de la vue). |
| `hover_outline` | `false` | L'outil de sélection entoure les objets sous la souris dans la vue 3D. |
| `keyboard_layout` | `"Auto"` | Touches de vol : `Auto` (par position : ZQSD en AZERTY, WASD en QWERTY), `Azerty` (par étiquette : Z Q S D, A descend, E monte), `Qwerty` (par étiquette : W A S D, Q descend, E monte). |

## `[edit]` : édition

**Paramètres > Édition**, les touches de grille et les menus **Map** et **Outils**.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `grid_size` | `16.0` | Taille de la grille globale, de 0,01 à 4096. Champ de la barre d'outils, `[` et `]`. |
| `snap_to_grid` | `true` | Magnétisme de la grille. **Map > Magnétisme de grille** (`Maj+W`). |
| `show_grid` | `true` | Affiche la grille 2D. **Map > Afficher la grille** (`Maj+G`). |
| `texture_lock` | `true` | Les textures suivent les brushes déplacés et tournés. **Outils > Textures > Verrouillage des textures** (`Maj+L`). |
| `texture_scale_lock` | `false` | Les textures suivent la mise à l'échelle des brushes. **Outils > Textures > Verrouillage de l'échelle des textures** (`Maj+K`). |
| `select_mode` | `"Groups"` | Ce qu'un clic sélectionne : `Groups`, `Objects`, `Solids`. **Édition > Mode de sélection**. |
| `default_material` | `"dev/dev_measuregeneric01b"` | Matériau des nouveaux brushes. |
| `default_texture_scale` | `0.25` | Échelle de texture des nouvelles faces. |
| `default_lightmap_scale` | `16` | Échelle de lightmap des nouvelles faces. |
| `default_point_entity` | `"info_player_start"` | Classe de l'outil entité quand rien d'autre n'a été choisi. |
| `default_brush_entity` | `"func_detail"` | Classe utilisée par **Lier à une entité**. |
| `undo_levels` | `500` | Nombre d'étapes d'annulation conservées. |
| `autosave_minutes` | `5` | Intervalle de la sauvegarde automatique en minutes (**Paramètres > Général > Sauvegarde automatique** ; 0 la désactive). |
| `autosave_keep` | `10` | Copies de sauvegarde automatique conservées par map. |
| `arrow_key_nudge` | `true` | Les flèches décalent la sélection dans les vues 2D. |
| `paste_offset` | `true` | Les objets collés sont décalés pour ne pas recouvrir les originaux. |
| `new_brush_color` | aucune | Couleur d'affichage `[r, g, b]` donnée aux nouveaux brushes ; aucune : la couleur par défaut aléatoire de Hammer. |
| `undo_restores_selection` | `true` | Les changements de sélection sont des étapes d'annulation : `Ctrl+Z` rétablit la sélection précédente (faces comprises). Ils ne marquent jamais la map comme modifiée. |
| `keep_displacements_on_cut` | `true` | Découper, Trancher, Creuser et Intersection gardent les déplacements sur les morceaux (exact sur les lignes de la grille, rééchantillonné ailleurs, coutures soudées) ; désactivé : les faces coupées redeviennent plates comme dans Hammer. **Paramètres > Édition**. |

## `[ui]` : interface

**Paramètres > Général**, le menu **Affichage > Panneaux** et la disposition elle-même.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `theme` | `"Dark"` | `Dark` (sombre) ou `Light` (clair). |
| `ui_scale` | `1.0` | Échelle de l'interface. |
| `accent_color` | absente | Couleur d'accent (RVB) des mises en valeur, de l'outil actif et de la sélection ; absente pour le style noir et blanc. |
| `show_texture_browser` | `true` | Onglet Textures du dock de droite. |
| `show_entity_properties` | `true` | Onglet Entité. |
| `show_visgroups` | `true` | Onglet Visgroups. |
| `show_outliner` | `false` | Onglet Arborescence. |
| `show_model_browser` | `false` | Onglet Modèles. |
| `show_console` | `true` | Panneau de la console. |
| `floating_panels` | `[]` | Panneaux latéraux ouverts dans des fenêtres séparées à l'intérieur de l'éditeur. Utilisez **Détacher le panneau** ou **Rattacher le panneau**. |
| `thumbnail_size` | `96.0` | Taille des vignettes du navigateur de textures. |
| `split_x`, `split_y` | `0.5`, `0.5` | Position des séparateurs de la grille 2x2 des vues (0 à 1). |
| `quadrants` | `[0, 1, 2, 3]` | Vue de chaque quart : 0 = 3D, 1 = Dessus, 2 = Face, 3 = Côté, 4 = logique 2D. |
| `background_image` | `""` | Image dessinée sur toute la fenêtre (vide : aucune). |
| `background_opacity` | `0.15` | Son opacité, de 0 à 1. |
| `background_offset` | `[0.0, 0.0]` | Son décalage par rapport à la position centrée, en points. |
| `background_zoom` | `1.0` | Son zoom par rapport à la taille qui couvre juste la fenêtre. |
| `background_hide_3d` | `true` | Ne pas la dessiner sur les vues 3D. |
| `native_frames` | `false` | Utilise les barres de titre et les cadres du système au lieu de ceux de Hammer Pro (bureau à distance, certains gestionnaires de fenêtres). Appliqué tout de suite. |
| `start_fullscreen` | `true` | Clé historique de **Démarrer agrandi**. Démarre agrandi dans la zone utile Windows, en gardant la barre des tâches accessible et les boutons réduire, agrandir/restaurer et fermer visibles. `F11` (**Agrandir la fenêtre**) bascule entre agrandi et restauré, en conservant la position et la taille précédentes. Appliqué au prochain démarrage. |
| `uv_window`, `portals_window` | absentes | Dernière position de l'éditeur d'UV et de la fenêtre des portails quand ce sont des fenêtres séparées (écrites par Hammer Pro). |

## `[game]` : jeu, contenu et définitions d'entités

**Paramètres > Jeu**. Les changements s'appliquent avec **Appliquer et recharger le contenu** ou à la fermeture de la fenêtre. Voir [Configurer le jeu](../getting-started/game-setup.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `game` | `"gmod"` | Le jeu pour lequel vous faites vos maps : un id du catalogue (`gmod`, `tf`, `portal2`...), `mod:<dossier>` pour un mod de `steamapps/sourcemods`, `dir:<chemin>` pour un autre dossier de jeu. Son contenu est monté, son FGD chargé et les maps sont compilées dans son format. Voir [Jeux cibles](../compiler/target-games.md). |
| `game_dirs` | `{}` | Dossiers de jeu choisis par vous (id du jeu = dossier qui contient `gameinfo.txt`), utilisés à la place de ceux trouvés dans Steam. |
| `gmod_root` | absente | Dossier de Garry's Mod (celui qui contient `garrysmod` et `bin`). Absente : détecté au démarrage. |
| `mount_addons` | `true` | Monte `garrysmod/addons` (dossiers et `.gma`). |
| `mount_workshop` | `false` | Monte les addons du Workshop (y compris les anciens `.bin`). |
| `mount_cfg` | `true` | Monte les jeux de `mount.cfg`. |
| `mount_server_content` | `true` | Monte le contenu téléchargé depuis les serveurs (`garrysmod/download`, `garrysmod/cache/workshop`). |
| `extra_game_dirs` | `[]` | Autres dossiers de jeu à monter. |
| `mount_app_ids` | `[]` | Jeux Steam dont le contenu est monté, par AppID, par ordre de priorité (par exemple `[240, 440]`). Chacun est trouvé dans les bibliothèques Steam et monté d'après son `gameinfo.txt`. Voir [Configuration du jeu](../getting-started/game-setup.md#jeux-steam-par-appid). |
| `custom_texture_dirs` | `[]` | Dossiers de textures personnalisés : tables avec `path`, `prefix` (préfixe des noms de matériaux quand le dossier n'est pas dans une arborescence `materials`) et `enabled`, `include_models`. Voir [Dossiers de textures personnalisés](../features/custom-texture-folders.md). |
| `extra_fgds` | `[]` | Fichiers FGD supplémentaires, chargés après celui du jeu. |
| `fgd_override` | absente | Remplace le FGD principal du jeu. |
| `map_dir` | absente | Dossier par défaut des nouvelles maps et de la fenêtre d'ouverture. |

## `[compile]` : compilation

**Paramètres > Compilation** et la [fenêtre de compilation](../compiler/compile-window.md). Les clés d'anciens réglages supprimés (chemins des compilateurs externes) sont ignorées.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `preset` | `"Normal"` | `Draft` (brouillon), `Normal`, `Final` ou `Custom` (les modes d'étape ci-dessous). |
| `lighting` | `"Ldr"` | `Ldr`, `Hdr` ou `Both`. |
| `run_bsp`, `run_vis`, `run_rad` | `true` | Étapes du préréglage Custom : géométrie, visibilité, éclairage. |
| `bsp_mode` | `"Normal"` | Géométrie du préréglage Custom : `Normal`, `OnlyEnts`, `OnlyProps`. |
| `vis_mode` | `"Normal"` | Visibilité du préréglage Custom : `Fast` ou `Normal`. |
| `rad_mode` | `"Normal"` | Éclairage du préréglage Custom : `Draft`, `Normal`, `Final`. |
| `pack_custom_content` | `true` | Intègre le contenu personnalisé au BSP (voir `pack_policy`). |
| `pack_policy` | `"ReleaseOnly"` | `ReleaseOnly` (seules les compilations Final et les publications intègrent le contenu ; les compilations d'itération utilisent l'addon de développement) ou `Always`. |
| `dev_addon` | `true` | Les compilations d'itération copient le contenu personnalisé dans `garrysmod/addons/hammerpro_dev` au lieu de l'intégrer. |
| `export_custom_materials` | `false` | Exporte les matériaux personnalisés. |
| `copy_to_maps` | `true` | Copie le BSP dans le dossier `maps` du jeu. |
| `bsp_beside_vmf` | `false` | Enregistre le BSP final à côté du VMF source et place les fichiers intermédiaires dans le cache de l'application. |
| `maps_dir_override` | absente | Dossier où copier le BSP (absente : `garrysmod/maps`). |
| `launch_game` | `true` | Lance le jeu après la compilation. |
| `launch_via_steam` | `false` | Lance le jeu par Steam. |
| `game_exe_path` | absente | Exécutable du jeu (absente : détecté). |
| `model_compiler` | absente | Programme compilateur de modèles utilisé par Propper, Combiner des props et Compiler un QC (absente : `bin/win64/studiomdl.exe`, sinon `bin/studiomdl.exe` du jeu). Un remplaçant comme NekoMDL fonctionne. **Paramètres > Compilation > Compilateur de modèles**. Un programme introuvable est signalé par son nom. MCP : `set_model_compiler` (avec `confirm`). |
| `model_compiler_args` | `""` | Arguments ajoutés pour le compilateur de modèles, après `-game <dossier> -nop4 -verbose` et avant le fichier QC. |
| `game_args` | `"-console -dev"` | Arguments du jeu ; les options de démarrage rapide du menu de lancement les modifient. |
| `threads` | absente | Threads du compilateur (absente : automatique, un cœur gardé pour l'éditeur avec `low_priority`). |
| `low_priority` | `true` | Garde un cœur pour l'éditeur. |
| `extra_bsp_args` | `""` | Arguments supplémentaires de `hammerpro --bspc`. |
| `extra_rad_args` | `""` | Arguments supplémentaires de `hammerpro --rad`. |
| `bsp_flag_choices`, `rad_flag_choices` | `{}` | Options forcées dans les menus d'options : option = `{ on = true, value = "4" }`. Voir [Options de compilation](../compiler/compile-options.md). |
| `save_before_compile` | `false` | Enregistre la map avant de compiler (la compilation utilise toujours un instantané et ne touche jamais au fichier). |
| `load_pointfile_on_leak` | `true` | Charge le chemin de la fuite quand la map fuit. |
| `stop_on_leak` | `true` | S'arrête après la géométrie quand la map fuit. |
| `compile_hidden` | `false` | Compile aussi les objets masqués dans l'éditeur. |
| `compile_cordon_only` | `true` | Avec un cordon actif, ne compile que la zone du cordon (sous le nom `<map>_cordon`). |
| `cordon_material` | `"TOOLS/TOOLSSKYBOX"` | N'est plus lue : le matériau de fermeture est `visibility.cordon_material`. |
| `smart_recompile` | `true` | Choisit la chaîne la moins coûteuse d'après ce qui a changé (Draft et Normal). |
| `background_precompile` | `true` à partir de 8 cœurs | Compile en arrière-plan quand l'éditeur est inactif. |
| `progressive` | `false` | Jouer tout de suite : compilation Draft, lancement, puis éclairage terminé en arrière-plan et map remplacée. |
| `progressive_preset` | `"Normal"` | Qualité de cet éclairage en arrière-plan (aucun réglage dans la fenêtre). |
| `history` | `true` | Garde un historique des durées par map. |
| `static_prop_lighting` | `true` | Autorise `-StaticPropLighting` quand la qualité le demande. |
| `static_prop_shadows` | `true` | Les props statiques projettent des ombres (désactivé : ajoute `-nostaticpropshadows`). |
| `expert` | `false` | Mode expert : exécute le profil choisi au lieu du préréglage. |
| `profiles` | `[]` | Profils experts : `name` et `steps` (`enabled`, `action`, `program`, `args`, `continue_on_error`). |
| `compilers` | `[]` | Vos compilateurs et vos changements des compilateurs détectés : `id` (`user:N` ou `detected:...`), `name`, `mode_flags`, `steps` (`kind` : `bsp`, `vis`, `rad`, `copy`, `game` ou `custom` ; `enabled`, `exe`, `options` (option vers valeur, `""` pour une case à cocher), `extra`, `args`, `workdir`, `continue_on_error`). Voir [Choisir un compilateur](../compiler/compilers.md). MCP le change avec `create_compiler` et `set_compiler`. |
| `compiler_by_game` | `{}` | Le compilateur de chaque jeu : id du jeu vers id du compilateur. Un jeu absent utilise le compilateur de Hammer Pro. MCP le change avec `select_compiler`. |
| `active_profile` | `0` | Index du profil choisi. |
| `separate_window` | `true` | La fenêtre Compiler et lancer est une fenêtre indépendante. |
| `window` | absente | Dernière position de cette fenêtre. |

## `[browser]` : navigateurs de ressources

Navigateurs de textures, de modèles, de sons et de particules, et **Paramètres > Entités et navigateurs**.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `favorites` | `[]` | Matériaux favoris. |
| `recent_materials` | `[]` | Matériaux utilisés récemment (64 au plus). |
| `hide_tool_textures` | `false` | Masque les textures outils dans le navigateur de textures. |
| `hidden_shaders` | `[]` | Shaders dont les matériaux sont masqués (`vertexlitgeneric` masque les matériaux des modèles). |
| `only_used_in_map` | `false` | N'affiche que les matériaux utilisés dans la map. |
| `last_filter` | `""` | Dernier texte de recherche. |
| `excluded_folders` | `[]` | Dossiers de matériaux masqués dans le navigateur (`dossier` ou `dossier/sous`). |
| `fgd_exclusions` | `true` | Masque aussi les dossiers exclus par le FGD (`@MaterialExclusion`). |
| `show_alpha` | `false` | Vignettes des matériaux avec alpha sur un damier. |
| `show_sound_browser` | `false` | Onglet du navigateur de sons. |
| `show_particle_browser` | `false` | Onglet du navigateur de particules. |
| `model_grid_view` | `true` | Navigateur de modèles en grille de rendus (désactivé : une liste). |
| `tile_size` | `96.0` | Taille des tuiles de modèles et de particules, de 48 à 256 points. |
| `gmod_only` | `false` | Les navigateurs de modèles, de sons et de particules ne listent que le contenu de base de Garry's Mod. |
| `sound_volume` | `0.8` | Volume des aperçus sonores, de 0 à 1. |
| `sound_autoplay` | `true` | Sélectionner un son le joue. |

## `[selection]`

**Paramètres > Sélection**. Voir [Outil de sélection](../features/selection-tool.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `clone_keeps_visgroups` | `true` | Les copies faites en glissant restent dans les visgroups des originaux. |
| `rotation_snap_deg` | `15.0` | Pas de rotation des poignées et des anneaux du gizmo, de 1 à 180 degrés. |
| `free_rotation_with_alt` | `false` | `Alt` (au lieu de `Ctrl`) tourne librement, sans le pas. |
| `handle_size` | `7.0` | Taille des poignées 2D, de 4 à 16 points. |
| `shift_arrow_clones` | `true` | `Maj` + flèches clone la sélection à un pas de grille ; le décalage d'une unité passe alors sur `Alt` + flèches. |
| `gizmo_local` | `false` | Le gizmo d'une seule entité ponctuelle suit ses angles (`Ctrl+J`). |
| `click_to_zoom` | `false` | L'outil caméra zoome les vues 2D au clic (`Ctrl` + clic dézoome). |
| `stretch_handles` | `true` | Des flèches sur les faces de la sélection, dans la vue 3D, l'étirent. |
| `box_confirm` | `false` | Sélection par cadre ajustable : un cadre tracé dans une vue 2D attend `Entrée` (comme Hammer++), ses poignées le redimensionnent, un glisser à l'intérieur le déplace, `Échap` l'abandonne. `false` (par défaut depuis la 2.0.3) sélectionne au relâchement. Les paramètres enregistrés par la 2.0.2 passent une fois à `false` lors de la mise à jour. |
| `random_yaw_min` | `0.0` | **Rotation Z aléatoire** : plus petite rotation, en degrés (-360 à 360). |
| `random_yaw_max` | `360.0` | **Rotation Z aléatoire** : plus grande rotation, en degrés (au plus 360 au-dessus de la plus petite). |
| `random_yaw_snap` | `0.0` | **Rotation Z aléatoire** : pas de la rotation, en degrés (0 : libre, jusqu'à 180). |
| `random_tilt` | `false` | **Rotation Z aléatoire** incline aussi un peu le tangage et le roulis. |
| `random_tilt_deg` | `3.0` | Inclinaison maximale du tangage et du roulis, en degrés (0 à 45). |

## `[brush]`

**Paramètres > Brushes**.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `reorient_primitives` | `true` | Les primitives dessinées dans la vue Face ou Côté suivent l'axe de profondeur de cette vue. |
| `clip_measurements` | `true` | L'outil de découpe affiche la longueur et l'angle de la ligne de coupe. |
| `clip_current_material` | `false` | Les faces coupées reçoivent le matériau actuel (désactivé : elles copient la face la plus proche). |
| `detail.split` | `"selection"` | **Transformer en func_detail** : un func_detail par sélection (`"selection"`) ou par ensemble de brushes qui se touchent (`"cluster"`). |
| `detail.check_sealing` | `true` | **Transformer en func_detail** garde dans le monde les brushes qui scellent la map (demande d'abord). |
| `detail.tint` | `false` | Les brushes func_detail prennent une teinte bleue en 3D et un contour bleu en 2D. |
| `detail.max_size`, `detail.steps`, `detail.trims`, `detail.pillars`, `detail.small`, `detail.complex`, `detail.in_selection` | `1024`, `true`, `true`, `true`, `true`, `true`, `false` | Filtres de **Sélectionner les candidats func_detail** (voir [func_detail](../features/func-detail.md)). |

## `[workflow]`

**Paramètres > Vues** (contrôles), **Paramètres > Général** (maps), **Paramètres > Clavier** (jeu de raccourcis), menus **Affichage** et **Fenêtre**.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `keymap_preset` | `"HammerPro"` | Jeu de raccourcis : `HammerPro`, `Hammer`, `HammerPlusPlus`. Vos propres raccourcis restent prioritaires. |
| `nav_scheme` | `"HammerPro"` | Contrôles de la caméra : `HammerPro` (clic droit glissé pour regarder, ZQSD/WASD pour voler, clic molette glissé pour faire défiler), `Hammer` (en plus : Espace + clic gauche glissé regarde, Espace + clic droit glissé fait défiler, clics gauche et droit glissés déplacent, les flèches regardent), `HammerPlusPlus` (les contrôles de Hammer, le clic molette regarde en 3D). |
| `invert_mouse_x` | `false` | Inverse l'axe horizontal de la souris. |
| `camera_smoothing` | `true` | Activer l'accélération fluide de la caméra 3D ; false rend les démarrages et arrêts immédiats. |
| `fly_response` | `0.07` | Secondes que met la caméra pour atteindre sa vitesse, de 0 à 0,5 (0 : immédiat). |
| `grid_step_factor` | `2.0` | Facteur de `[` et `]`, de 1,25 à 10. Avec 2, les puissances de deux de 1/8 à 4096. |
| `ctrl_wheel` | `"GridSize"` | `Ctrl` + molette dans une vue 2D : `GridSize` (taille de grille) ou `ZoomAll2d` (zoom de toutes les vues 2D). |
| `link_2d_zoom` | `false` | Zoomer une vue 2D zoome les autres. |
| `drag_autopan` | `false` | Faire glisser quelque chose avec le bouton gauche près du bord d'une vue 2D ou au delà la fait défiler, d'autant plus vite que le curseur va loin, comme Hammer. |
| `units` | `"Units"` | Longueurs de la barre d'état : `Units`, `Inches`, `FeetInches`, `Meters` (une unité = 2,54 cm). |
| `show_view_info` | `false` | Images par seconde, position et angles de la caméra dans la vue 3D (`O`). |
| `show_bounds` | `false` | Boîtes englobantes jaunes des objets visibles. Pas de raccourci (`I` est le décalage interactif) : activez-les par **Affichage > Options d'affichage > Boîtes englobantes**. |
| `show_toolbar`, `show_tool_palette`, `show_status_bar` | `true` | **Affichage > Panneaux**. |
| `tool_palette_columns` | `1` | La palette d'outils sur une ou deux colonnes (`2` : boutons côte à côte). Clic droit sur la palette, **Affichage > Panneaux > Palette d'outils sur deux colonnes** ou **Paramètres > Général > Palette d'outils**. |
| `toolbar_rows` | `1` | `2` : la barre d'outils passe sur une deuxième ligne quand la fenêtre est trop étroite au lieu de ranger des groupes dans **»**. Clic droit sur la barre d'outils, **Affichage > Panneaux > Barre d'outils sur deux lignes** ou **Paramètres > Général > Barre d'outils**. |
| `toolbar_order` | `[]` | Ordre des groupes de la barre d'outils (identifiants `file`, `edit`, `grid`, `groups`, `hide`, `cordon`, `display`, `show`, `render`) ; les groupes non cités suivent dans l'ordre par défaut, `compile` reste toujours tout à droite. Vide : l'ordre par défaut. |
| `toolbar_hidden` | `[]` | Groupes de la barre d'outils masqués (les identifiants ci-dessus et `compile`) et boutons masqués (l'identifiant d'action du bouton, ou `grid_size`, `render_mode`, `props_2d`, `primitive_stats` ; Compiler se masque par son identifiant de groupe `compile`). Clic droit sur la barre d'outils > **Personnaliser la barre d'outils** ; **Réinitialiser la barre d'outils** la vide. |
| `show_history` | `false` | Onglet de l'historique des modifications. |
| `layouts` | `{}` | Dispositions enregistrées (**Fenêtre > Disposition > Enregistrer la disposition...**), 32 au plus : `quadrants`, `split_x`, `split_y`, `maximized`, `panels`. |
| `open_in_new_tab` | `true` | Les maps ouvertes ont leur propre onglet (désactivé : elles remplacent la map actuelle). |
| `team_live_edits` | `true` | Team Mapping : envoyer les coups de pinceau et déplacements aux autres participants pendant qu'ils se font (désactivé : seulement à la fin). Voir [Team Mapping](../features/team-mapping.md). |
| `team_share_scripts` | `true` | Hôte Team Mapping : partager les entités et entrées de script et de commande (`lua_run`, `logic_script`, `point_servercommand`, `point_clientcommand`, `Command`, `RunScriptCode`, `RunScriptFile`, `RunPassedCode`). Désactivé : l'hôte garde les siennes sur son PC et refuse celles que les autres ajoutent ou modifient. Voir [Team Mapping](../features/team-mapping.md). |

## `[instances]`

**Paramètres > Instances** et **Outils > Instances**. Voir [Instances](../features/instances.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `display` | `"Tinted"` | Contenu des `func_instance` : `Hidden` (masqué), `Tinted` (teinté), `Normal`. |
| `folder` | `"instances"` | Dossier des nouvelles instances, relatif au dossier de la map (ou absolu). |
| `collapse_skips_hidden` | `true` | La fusion laisse de côté les objets masqués dans les fichiers d'instance. |
| `outline_selected` | `true` | Entoure le contenu des instances sélectionnées et affiche leur fichier. |
| `show_context` | `true` | Dessine la map autour d'une instance en cours de modification, teintée et en lecture seule. |

## `[visibility]`

**Paramètres > Cordons et visgroups**.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `cordon_material` | `"tools/toolsskybox"` | Matériau des brushes qui ferment une compilation de cordon. |
| `shade_outside_2d` | `true` | Assombrit la zone hors des cordons dans les vues 2D. |
| `shade_opacity` | `0.45` | Opacité de cet assombrissement, de 0 à 1. |
| `show_cordon_names` | `true` | Noms des cordons dans les vues 2D. |
| `move_remove_others` | `true` | **Déplacer vers un visgroup** retire les objets de leurs autres visgroups. |
| `move_hide` | `false` | **Déplacer vers un visgroup** masque ensuite le visgroup. |

## `[helpers]`

**Paramètres > Aides** et **Affichage > Options d'affichage**.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `helper_scope` | `"Selected"` | Aides (rayons, cônes, lignes vers les cibles) des entités sélectionnées (`Selected`) ou de toutes (`All`). Les liens de chemin des aides `line()` et `cylinder()` (`path_track`, `path_corner`) sont dessinés pour toutes les entités dans les deux modes, comme dans Hammer. |
| `connections` | `"Selected"` | Lignes d'entrées/sorties : `Off`, `Selected`, `All`. |
| `show_overlays` | `true` | Dessine les `info_overlay` sur leurs faces. |
| `show_editor_objects` | `true` | Icônes des entités et géométrie des aides. |
| `show_entity_names` | `true` | Noms des entités ponctuelles dans les vues 2D. |
| `show_brush_centers` | `true` | Petit x au centre des brushes visibles en 2D, de taille constante à l’écran. |
| `show_ropes` | `true` | Dessine les cordes comme elles pendent en jeu. |
| `simulate_ropes` | `false` | Anime les cordes pendant leur stabilisation et sous env_wind, sauf si NoWind est activé. |

## `[mesh]`

**Paramètres > Sommets et displacements**.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `uv_lock` | `false` | Outil vertex : les textures suivent les sommets déplacés. |
| `live_preview` | `true` | Outil vertex : les solides montrent leur nouvelle forme pendant le glisser. |
| `four_way_blends` | `true` | La peinture d'alpha peut peindre les 4 textures des matériaux à mélange 4 voies. |
| `noise_min`, `noise_max` | `-4.0`, `4.0` | Plage de hauteur de **Ajouter du bruit aux displacements...**. |
| `mesh_export_meters` | `false` | **Displacement en maillage** écrit des mètres au lieu des unités Source. |
| `mesh_export_z_up` | `false` | **Displacement en maillage** écrit Z vers le haut au lieu de Y vers le haut. |
| `disp_tool` | `{}` | Toutes les options du panneau de l'outil displacement, par nom (écrites par l'outil). |

## `[entities]`

**Paramètres > Entités et navigateurs** ; `[entities.path]` vient des options de l'outil chemin.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `io_esc_delimiter` | `true` | Les nouvelles sorties utilisent le séparateur ESC (les paramètres peuvent contenir des virgules). |
| `paste_targetname` | `false` | **Coller les keyvalues** colle aussi le nom. |
| `texture_decimals` | `4` | Décimales des champs de l'édition de faces, de 0 à 6. |
| `texture_shift_step` | `1.0` | Pas de glisser des champs de décalage. |
| `texture_scale_step` | `0.005` | Pas de glisser des champs d'échelle. |
| `crowbar_exe` | absente | Exécutable Crowbar choisi dans l'inspecteur de modèles. |
| `path.class` | `"path_corner"` | Classe des nouveaux points de chemin. |
| `path.base_name` | `"path"` | Nom de base des points (`path_1`, `path_2`...). |
| `path.close_loop` | `false` | Terminer un chemin relie le dernier point au premier. |
| `path.relink_on_delete` | `true` | Supprimer des points relie leurs voisins. |
| `path.surface_offset` | `0.0` | Hauteur au-dessus de la surface des points posés en 3D, de -1024 à 1024. |

## `[render]`

**Paramètres > Rendu** et **Affichage > Options d'affichage**. Voir [Modes de rendu et affichage](../features/render-modes-display.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `grid_3d` | `false` | Grille 3D sur les faces des brushes (`P`). |
| `grid_3d_opacity` | `0.45` | Son opacité. |
| `grid_3d_fade_cells` | `64.0` | Elle s'estompe au-delà de ce nombre de cases. |
| `near_plane` | `1.0` | Distance de découpe proche, de 0,5 à 16 unités. |
| `model_distance` | `0.0` | Les modèles plus loin que cette distance sont des boîtes (0 : sans limite). |
| `lod_distances` | `[0.0, 0.0]` | Distances des LOD 1 et 2 pour les modèles lourds (0 : toujours le modèle complet). Aucun réglage dans la fenêtre. |
| `models_in_2d` | `"SelectedOnly"` | `Boxes`, `Wireframe`, `SelectedOnly`. |
| `collision_models` | `false` | Modèles de collision par-dessus les modèles. **Affichage > Diagnostics > Modèles de collision**. |
| `collision_only` | `false` | N'affiche que les modèles de collision. |
| `fill_selected_2d` | `true` | Remplit les faces sélectionnées dans les vues 2D. |
| `fill_selected_2d_opacity` | `0.2` | Opacité de ce remplissage. |
| `camera_in_2d` | `true` | Dessine la caméra 3D dans les vues 2D. |
| `vertex_dots_2d` | `true` | Sommets des brushes en points quand on zoome. |
| `icons_in_2d` | `true` | Icônes des entités dans les vues 2D. |
| `icons_in_2d_px` | `16.0` | Leur taille en pixels. |
| `use_visgroup_colors` | `false` | Les couleurs filaires 2D viennent des visgroups. |
| `object_colors_2d` | `true` | Les vues 2D dessinent les objets dans leur couleur d'affichage (voir [Couleurs d'affichage](../features/display-colors.md)) ; désactivé : l'aspect classique. |
| `object_colors_3d` | `false` | La vue 3D utilise aussi les couleurs d'affichage en mode filaire et plat. |
| `highlight_invalid` | `true` | Brushes invalides en rouge. |
| `render_effects` | `true` | Aperçu de render amount, mode, couleur, groupes de corps, animation et échelle. |
| `sky_preview` | `true` | Skybox 2D derrière la vue 3D. |
| `skybox_3d` | `true` | Skybox 3D autour de `sky_camera` (`Alt+V`). |
| `sky_see_through` | `false` | Laisse de côté les faces `toolsskybox` pour voir le ciel au travers. |
| `water_preview` | `true` | Eau dessinée comme en jeu. |
| `detail_preview` | `false` | Herbe et objets de détail des matériaux `%detailtype` répartis autour de la caméra 3D (voir [Aperçu des detail props](../features/detail-props-preview.md)). |
| `water_detail` | `"ReflectAll"` | Comme l'option vidéo Détail de l'eau du jeu : `ReflectAll` (r_waterforceexpensive et r_waterforcereflectentities : eau coûteuse à toute distance qui reflète le monde, les props, les entités de brushes, le ciel et la skybox 3D dessinés une seconde fois en miroir par rapport à sa surface), `ReflectWorld` (coûteuse près de la caméra, bon marché au-delà de `$cheapwaterenddistance`, props et entités de brushes seulement avec `$reflectentities` ; `$reflectskyboxonly` et `$reflect3dskybox 0` sont respectés), `Simple` (eau bon marché partout). |
| `fog_preview` | `true` | Aperçu du brouillard d'un `env_fog_controller` (Fog Enable, couleurs, mélange, début, fin, densité max, farz) dans les vues 3D et la vue joueur. |
| `fog_volume_preview` | `true` | Entourer et remplir les brushes `fog_volume` de leur brouillard (celui du contrôleur, ou un aspect par défaut avec une étiquette d'avertissement) ; la vue joueur prend le brouillard du contrôleur dedans. |
| `fog_volume_texture` | `true` | Dessiner le matériau des brushes `fog_volume` dans la vue 3D. |
| `fish_preview` | `true` | Dessiner les poissons des entités `func_fish_pool`. |
| `fish_swim` | `true` | Les poissons de l'aperçu nagent. |
| `normal_maps` | `true` | Les normal maps donnent leur relief aux surfaces. |
| `player_compiled_lighting` | `true` | La vue joueur utilise les lightmaps de la map compilée. |
| `cubemaps` | `true` | Les matériaux `$envmap` reflètent leur cubemap. |
| `cubemaps_auto` | `true` | Les cubemaps sont redessinées après les modifications de la map. |
| `light_shadows` | `16` | Lumières ponctuelles et spots avec ombres en mode éclairé, de 0 à 64 (les plus brillantes en vue d'abord). |
| `lit_indirect` | `true` | Le mode éclairé reçoit sa lumière du ciel et la lumière rebondie par les autres surfaces d'une cuisson de la map en arrière-plan avec le compilateur. |
| `model_lods` | `true` | Chaque prop est dessiné au niveau de détail que le jeu choisit pour lui, d’après les distances de bascule de son modèle et la taille qu’il occupe à l’écran. Désactivé : le niveau le plus fin de chaque modèle est toujours dessiné. |
| `game_pixel_grid` | `true` | Les vues 3D échantillonnent leurs pixels sur la même grille que le jeu (Direct3D 9), à un demi pixel de la grille de Vulkan et de Direct3D 12, pour qu’une capture de l’aperçu se superpose à une capture du jeu. |
| `sun_glow` | `true` | Le sprite du soleil et le halo de l’entité `env_sun` sont dessinés dans les vues 3D comme le jeu (direction, `size`, `overlaysize`, `rendercolor`, `overlaycolor`, `material`, `overlaymaterial`, `HDRColorScale`, le fondu sur les 40 degrés autour du centre de la vue et le fondu derrière ce qui cache le soleil). |
| `light_preview_hdr` | `true` | Le mode éclairé prend les valeurs de lumière HDR (`_lightHDR`, `_lightscaleHDR`, `_ambientHDR`, `_AmbientScaleHDR`). |
| `projected_textures` | `true` | Les entités `env_projectedtexture` (les 16 premières) projettent leur texture et leur lumière en mode éclairé, en aperçu précalculé et en vue joueur. |
| `sky_occlusion` | `true` | En mode éclairé, la lumière du ciel `_ambient` n'atteint que ce qui voit le ciel. |
| `tone_mapping` | `"Auto"` | Comment les vues éclairées (mode éclairé, aperçu précalculé, vue joueur avec son éclairage HDR compilé) transforment la lumière en pixels comme le jeu : `Auto` adapte l'exposition à l'image comme le moteur (`mat_autoexposure_min`, `mat_autoexposure_max`, `mat_tonemap_percent_target`), `Fixed` utilise `exposure`, `Off` montre la lumière telle quelle. **Outils > Options... > Rendu > Tone mapping HDR**. |
| `exposure` | `1.0` | Le facteur d'exposition du mode `Fixed`, de 0,05 à 16 (`mat_force_tonemap_scale`). |
| `exposure_min` | `0.5` | L'exposition automatique ne descend jamais sous cette valeur, de 0,05 à 16 (`mat_autoexposure_min`). |
| `exposure_max` | `2.0` | L'exposition automatique ne dépasse jamais cette valeur, de 0,05 à 16 (`mat_autoexposure_max`). |
| `bloom` | `1.0` | Intensité de la lueur autour des surfaces claires, de 0 à 8 (`mat_bloom_scalefactor_scalar`) ; 0 la désactive. |
| `map_tonemap` | `true` | L'`env_tonemap_controller` de la map règle la plage d'exposition et l'échelle du bloom (`SetAutoExposureMin`, `SetAutoExposureMax`, `SetBloomScale` depuis son `logic_auto`), comme en jeu. |
| `prop_shadows` | `"Auto"` | Ce avec quoi les `prop_static` projettent leurs ombres en mode éclairé : `Auto` suit la compilation (triangles du modèle avec `-StaticPropPolys`, modèle de collision sinon, aucune avec `-nostaticpropshadows`), `Mesh` les triangles du modèle, `Collision` le modèle de collision. |
| `animate_materials` | `true` | Matériaux animés et sprites en mouvement. |
| `model_fade` | `false` | Les props disparaissent progressivement entre leurs distances de fondu. |
| `force_opaque` | `false` | Matériaux translucides dessinés opaques. |
| `walkable` | `false` | Teinte les surfaces trop pentues pour marcher. |
| `radius_culling` | `false` | Masque ce qui est au-delà de `cull_radius`. |
| `cull_radius` | `4096.0` | Rayon de la limite de distance. |
| `show_displacements` | `true` | Affiche les displacements. |
| `displacements_in_2d` | `true` | Dessine les displacements dans les vues 2D. |
| `displacements_only` | `false` | N'affiche que les displacements. |
| `displacements_2d` | `"Mesh"` | `Mesh`, `SelectedMesh`, `Outline`. |
| `displacements_2d_views` | `["Global", ...]` | Réglage propre à chaque quart, à la place de `displacements_2d`. |
| `disp_source_2d` | `true` | Contour léger du brush d'origine des displacements en 2D. |
| `disp_walkable` | `false` | Colore les triangles des displacements selon leur praticabilité. |
| `disp_walk_mode` | `Source` | Couleurs de l'affichage de praticabilité : `Source` (trois classes du moteur), `Limit` (vert jusqu'à l'angle, rouge au-dessus) ou `Gradient` (bandes autour de l'angle). |
| `disp_walk_angle` | `45.573` | Angle praticable en degrés pour les modes `Limit` et `Gradient` (1 à 89) ; 45,573 est la limite de Source. |
| `occlusion_culling` | `true` | Saute les blocs cachés derrière une géométrie plus proche. |
| `selection_outline` | `false` | Contour de la sélection à travers les murs. |
| `grid_dotted` | `false` | Grille 2D en points. |
| `grid_highlight_64` | `false` | Lignes plus claires toutes les 64 unités. |
| `grid_min_px` | `4.0` | Masque les niveaux de grille 2D plus serrés que ce nombre de pixels. |
| `hide_nodraw`, `hide_clips`, `hide_triggers`, `hide_hints` | `false` | Groupes de textures outils masqués dans la vue 3D. |
| `max_fps` | `0` | Limite d'images par seconde (0 : aucune). |
| `unfocused_fps` | `30` | Images par seconde quand la fenêtre n'a pas le focus (0 : sans limite). |
| `animation_fps` | `30` | Images par seconde des matériaux animés, des sprites de détail qui ondulent, des effets de particules et des lumières qui clignotent quand rien d'autre ne bouge (0 : chaque image, 240 au plus). |
| `animation_idle_pause_s` | `120` | Secondes sans souris ni clavier après lesquelles les animations s'arrêtent jusqu'à la prochaine action (0 : jamais). |
| `animate_in_background` | `false` | Continuer les animations quand une autre fenêtre est active. |
| `vsync` | `true` | Attend le rafraîchissement de l'écran (s'applique immédiatement). |
| `vram_budget_mb` | `0` | Mémoire graphique que Hammer Pro peut utiliser, en Mo (0 : automatique, 90 % de ce que le système donne au programme). Au-delà de 85 %, les maps des autres onglets sont retirées de la carte graphique, au-delà du budget les textures sont affichées en plus basse résolution. Voir [Mémoire insuffisante](../troubleshooting/out-of-memory.md). |
| `keep_background_maps_on_gpu` | `true` | Garde les maps des autres onglets sur la carte graphique tant qu'il y a de la place, pour un changement d'onglet instantané. |
| `texture_streaming` | `true` | Détail des textures selon la distance : les textures utilisées seulement loin de toutes les vues sont gardées à une taille réduite qui rend pareil à cette distance (des niveaux de mip entiers, jamais moins de 128 pixels), et reviennent en pleine taille avant que la caméra ne s'approche. L'écran de chargement d'une map reste jusqu'à ce que chaque texture proche de la caméra soit en pleine taille. Désactivé : toutes les textures restent en pleine taille. Voir [Performances de l'éditeur](../troubleshooting/performance.md#détail-des-textures-selon-la-distance). |
| `performance_mode` | `Auto` | `Auto` et `Off` dessinent l'aperçu en pleine qualité. `Auto` baisse la qualité d'un cran (avec un avis et un bouton Rétablir la qualité maximale sur la vue 3D) seulement quand la vue 3D reste sous 20 images par seconde pendant plusieurs secondes. Avant cela, `Auto` adapte aussi le coût des aperçus d'éclairage à une carte graphique lente, un niveau à la fois et dans les deux sens (voir [Performances de l'éditeur](../troubleshooting/performance.md#adaptation-fine-de-léclairage-auto)) ; une carte qui suit garde toujours le rendu complet. `Low` (résolution à 75 %, sans MSAA ni bloom, 4 lumières avec ombres, ombre du soleil en 2048, textures jusqu'à 1024 pixels) et `VeryLow` (résolution à 50 %, sans ombres des lumières, reflets, normal maps ni volumes de brouillard, textures jusqu'à 512 pixels) la baissent toujours. **Paramètres > Rendu > Performance**, `set_setting render.performance_mode`. Voir [Performance de l'éditeur](../troubleshooting/performance.md). |
| `lit_quality` | `Auto` | Nombre de lumières que chaque partie de la vue 3D dessine en mode éclairé et dans l'aperçu calculé. `High` : jusqu'à 128 (le rendu complet, toujours). `Balanced` : seulement les 32 plus fortes de chaque partie, plus rapide sur les maps avec des centaines de lumières, quelques lumières faibles sont omises. `Auto` : `Balanced` seulement pendant que la caméra bouge, `High` dès qu'elle s'arrête, une image immobile est donc toujours complète. **Paramètres > Rendu > Performance > Qualité de l'aperçu d'éclairage**, `set_setting render.lit_quality`. Voir [Performances de l'éditeur](../troubleshooting/performance.md#aperçus-déclairage-à-pleine-vitesse). |
| `render_scale` | `1.0` | Dessine la vue 3D à cette part de sa taille (0,25 à 1) puis l'étire sur la fenêtre. 1.0 est la qualité maximale. |
| `perf_hud` | `false` | Affiche dans la vue 3D les images par seconde, le temps d'image, le temps CPU et GPU par étape, la mémoire et le niveau de qualité. |

## `[lighting]`

**Paramètres > Éclairage** et l'onglet Éclairage. Les options du précalcul lui-même (échantillons, rebonds...) appartiennent à chaque map (son bloc Hammer++ `light_plus`). Voir [Aperçu de l'éclairage](../features/lighting-preview.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `auto_bake` | `true` | Lance un précalcul quand l'aperçu précalculé est activé et que rien n'est encore calculé. Toute la carte par défaut ; le mode régional optionnel concerne ce démarrage automatique. |
| `threads` | `0` | Threads du précalcul (0 : tous les cœurs sauf un quart). Au moins deux cœurs restent toujours libres : un nombre plus grand est limité à tous les cœurs sauf deux. |
| `max_luxels_millions` | `16` | Luxels calculés en pleine résolution, de 1 à 64 millions. |
| `auto_scope` | `false` | Mode régional optionnel au démarrage automatique de l'aperçu sur les grandes cartes. Sa région suit la caméra ; **Précalculer l'éclairage** calcule toujours toute la carte. |
| `auto_scope_luxels_millions` | `2` | Seuil du mode régional automatique optionnel, de 1 à 64 millions de luxels. |
| `auto_scope_radius` | `3072` | Rayon de la région automatique optionnelle, de 256 à 32768 unités. Distinct du rayon manuel de l'onglet Éclairage. |
| `visible_first` | `true` | Sur les maps de 6000 solides ou plus, la zone autour de la caméra est précalculée d'abord, puis toute la map est précalculée en arrière-plan et la remplace. |
| `prepare` | `true` | Une fois la map ouverte et les modifications en pause, toute la map est compilée et éclairée avec la priorité la plus basse sans être dessinée, pour que l'aperçu précalculé s'affiche aussitôt à l'activation. Pas pendant que la fenêtre est réduite ou qu'une autre fenêtre est active. Demande `auto_bake`. |
| `priority` | `"BelowNormal"` | Priorité processeur des précalculs que vous attendez : `Idle`, `BelowNormal` ou `Normal`. La préparation en arrière-plan et l'éclairage des props tournent toujours en `Idle`. |
| `pause_minimized` | `true` | Arrêter le précalcul en cours et les mises à jour pendant que la fenêtre est réduite ; ils reprennent à son retour. |
| `pause_unfocused` | `false` | La même chose pendant qu'une autre fenêtre est active. |
| `drag_pause_ms` | `120` | Pendant le déplacement d'une lumière, la recalculer dès que la souris reste immobile ce temps (0 à 2000 ms ; 0 : en continu) et au relâchement. |
| `upload_mb_per_frame` | `4` | Données de lightmap envoyées à la carte graphique par image, de 1 à 64 Mo. |
| `tiny_scale` | `4` | Échelle de lightmap à partir de laquelle (ou en dessous) une face est jugée minuscule, de 1 à 64. |
| `budget_styles` | `1` | Styles de lumière comptés par le budget de lightmap, de 1 à 4. |
| `show_budget` | `true` | Jauge du budget de lightmap dans la barre d'état. |
| `quality` | `"Draft"` | `Draft`, `Normal` ou `Final`. |

Les valeurs existantes enregistrées de `auto_scope` sont conservées. Si le champ est absent, le défaut est `false`. **Précalculer autour de la caméra** reste un précalcul manuel d'une région fixe ; le mode régional automatique suit la caméra quand elle quitte la moitié centrale et reste immobile une seconde.

Le résultat global est réutilisé pendant la navigation. La géométrie, les lumières, les matériaux et les paramètres de calcul affectent sa validité ; les déplacements de caméra, la sélection, la luminosité de l'aperçu, la correction colorimétrique et le délai de mise à jour ne l'invalident pas. Les modifications de lumières sont recalculées de façon incrémentale dans le processus d'éclairage prêt (seules les lumières modifiées et la lumière rebondie) ; les mises à jour dynamiques planifient un nouveau précalcul après les autres changements pertinents ; sinon l'onglet Éclairage indique que le résultat est périmé et propose **Précalculer à nouveau**.

## `[placement]`

**Paramètres > Prefabs**. `[placement.sprinkle]` vient des options de l'outil sprinkle. Voir [Prefabs](../features/prefabs.md) et [Outil sprinkle](../features/sprinkle-tool.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `show_prefabs` | `true` | Onglet Prefabs. |
| `prefab_library` | absente | Dossier des nouveaux prefabs (absente : `prefabs` dans le dossier des paramètres). |
| `prefab_dirs` | `[]` | Autres dossiers de prefabs (lecture seule). |
| `game_prefabs` | `true` | Liste aussi les prefabs de l'installation du jeu. |
| `prefab_tile` | `96.0` | Tuiles des prefabs, de 56 à 192 points. |
| `group_prefabs` | `true` | Groupe les objets d'un prefab inséré. |
| `sprinkle.kind` | `""` | Type de sprinkle choisi. |
| `sprinkle.mode` | `"Add"` | `Add`, `Subtract`, `Replace`, `Overwrite`. |
| `sprinkle.density` | `50.0` | Probabilité en pourcentage par case. |
| `sprinkle.radius` | `128.0` | Rayon du pinceau, de 8 à 4096. |
| `sprinkle.use_grid` | `false` | Utilise la grille de l'éditeur au lieu de celle de la définition. |
| `sprinkle.random_yaw` | `true` | Lacet aléatoire. |
| `sprinkle.random_offset` | `50.0` | Déplacement aléatoire en pourcentage d'une demi-case. |
| `sprinkle.z_offset` | `0.0` | Distance le long de la normale de la surface. |
| `sprinkle.align_to_normal` | `false` | Incline les entités selon la surface. |
| `sprinkle.extra_dirs` | `[]` | Autres dossiers de définitions de sprinkle. |
| `overlap_check` | `false` | Vérifier les problèmes liste aussi les props qui chevauchent d'autres objets. |
| `problem_checks` | `true` | Vérifier les problèmes liste les props flottants et enfoncés (voir [Placement sur les formes réelles](../features/real-shape-placement.md)). |
| `auto_settle_on_create` | `false` | Les nouveaux props créés avec l'outil entité, le navigateur de modèles ou MCP descendent sur la surface en dessous (même étape d'annulation). |
| `show_real_dimensions` | `false` | Dimensions réelles de la sélection dans la barre d'état, le panneau des propriétés et les vues (`Alt+M`). |

## `[launch]`

Menu de lancement de la fenêtre de compilation et **Paramètres > Jeu > Tests en jeu**. Les options de démarrage rapide sont des options de `compile.game_args`. Voir [Lancer le jeu](../features/game-launch.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `timing` | `"AtEnd"` | Lance le jeu à la fin de la compilation (`AtEnd`) ou dès son début (`AtStart`). |
| `reuse_running` | `true` | Charge la map dans le jeu déjà lancé. |
| `skip_unused_content` | `false` | Ne monte pas le Workshop et les addons si la map n'en utilise rien. |
| `load_report` | `false` | Indique le temps de chargement de la map par le jeu (`-condebug`). |
| `spawn_at_camera` | `false` | Place le joueur à la caméra de l'éditeur (nécessite l'addon passerelle). |
| `args_version` | `0` | Interne : version de la mise à jour unique des arguments du jeu. |

## `[content]`

La fenêtre **Construire et enregistrer en contenu** et l'onglet Ressources de la map. Voir [Construire et publier](../features/build-content-release.md) et [Protect your BSP](../features/bsp-protection.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `mode` | `"WorkshopAddon"` | `WorkshopAddon` (BSP, arborescence de contenu et `.gma`), `SelfContained` (tout dans le BSP), `ServerKit` (BSP autonome et arborescence FastDL). |
| `compile_first` | `true` | Lance d'abord une compilation Final. |
| `compress_pakfile` | `false` | Compresse le pakfile en LZMA (Garry's Mod affiche alors un avertissement par entrée). |
| `lzma_level` | `6` | Niveau LZMA, de 0 à 9. |
| `pack_workshop` | `true` | Livre le contenu venant d'éléments du Workshop (sinon ils sont listés comme éléments requis). |
| `pack_mounted` | `false` | Livre le contenu des jeux de `mount.cfg`. |
| `keep_cubemaps` | `true` | Recopie les cubemaps construites du BSP précédent. |
| `game_session` | `false` | Construit les cubemaps et le nav mesh en jeu. |
| `write_gma` | `true` | Écrit le `.gma`. |
| `open_folder` | `false` | Ouvre le dossier de sortie à la fin. |
| `protect` | `false` | Protect your BSP. |
| `protect_watermark` | `""` | Filigrane de la copie protégée. |
| `protect_strip_original_faces` | `true` | Vide le lump des faces d'origine. |
| `protect_lock` | `false` | Verrou serveur. |
| `protect_lock_key` | `""` | Clé secrète du verrou serveur (seule son empreinte va dans la map). |
| `protect_lock_block` | `false` | Bloque le jeu sur un serveur non autorisé. |
| `protect_randomize` | `true` | Brouillage des matériaux. |
| `protect_shuffle_lumps` | `true` | Ordre physique aléatoire des lumps. |
| `protect_junk_gaps` | `true` | Trous de bourrage aléatoires entre les lumps. |
| `protect_decoys` | `true` | Blocs leurres non référencés. |
| `protect_scramble_header` | `true` | Champs d'en-tête ignorés par le jeu rendus aléatoires. |
| `protect_shuffle_game_lumps` | `true` | Répertoire des game lumps mélangé. |
| `protect_random_pak` | `true` | Pakfile à une position aléatoire. |
| `protect_reorder_tables` | `true` | Permute plans, sommets, arêtes, texinfo et texdata. |
| `protect_reorder_world` | `true` | Permute brushes et faces (les listes des feuilles restent dans l'ordre). |
| `protect_layout_seed` | `""` | Graine du brouilleur de structure (vide : aléatoire à chaque compilation). |
| `show_map_assets` | `false` | Onglet Ressources de la map. |
| `collect_folder` | `""` | Dernier dossier de sortie de **Collecter**. |

## `[health]`

**Paramètres > Santé de la map**. Voir [Vérifier les problèmes](../features/check-problems.md) et [Fuites et pointfiles](../features/leaks-pointfiles.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `live_checks` | `true` | Vérifie les problèmes de la map en arrière-plan. |
| `check_delay_ms` | `1500` | Vérifie la map quand les modifications marquent une pause de cette durée (de 0 à 10000 ms), jamais pendant un déplacement. Les vérifications tournent avec la priorité la plus basse sur un tiers des cœurs ; après une modification seuls les props proches sont vérifiés à nouveau pour les problèmes de placement. |
| `pause_minimized` | `true` | Arrête les vérifications de fond pendant que la fenêtre est réduite ; elles reprennent à son retour. |
| `live_leak_check` | `true` | Vérification instantanée des fuites en arrière-plan. |
| `show_leak_path` | `true` | Dessine le chemin de cette fuite. |
| `limit_profile` | `"auto"` | Limites des estimations : `auto` (le jeu de la map) ou l'id d'un profil. |
| `visible_only` | `false` | Ne liste que les problèmes des objets visibles. |
| `portal_width` | `1.0` | Épaisseur des lignes de portails, de 1 à 6 pixels. |
| `leaf_grid` | `false` | Dessine la grille de 1024 unités sur laquelle les leafs sont découpées. |
| `leaf_width` | `1.0` | Épaisseur de ses lignes, de 1 à 6. |
| `offer_portals` | `true` | Propose de charger le nouveau fichier de portails après une compilation. |
| `portal_heat` | `false` | Colore les portails selon le coût de rendu. |
| `portal_xray` | `true` | Montre les portails à travers les murs en 3D. |
| `warn_duplicates` | `true` | Avertit quand un nouveau brush ou une nouvelle entité est une copie exacte d'un autre au même endroit. |

## `[lab]`

**Paramètres > Nav mesh et particules** et les options de Propper et de la conversion inverse. Voir [Nav mesh](../features/nav-mesh.md), [Particules](../features/particles.md), [Propper](../features/propper.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `show_nav_mesh` | `false` | Dessine le nav mesh. |
| `nav_connections` | `true` | Dessine les connexions entre zones. |
| `nav_fill` | `true` | Remplit les zones en 3D. |
| `show_particles` | `true` | Joue les effets des `info_particle_system`. |
| `particles_selected_only` | `false` | Seulement les systèmes sélectionnés. |
| `show_entity_fx` | `true` | Aperçu de `func_smokevolume`, `env_embers`, `func_dustcloud` et `func_dustmotes`. |
| `color_correction_preview` | `false` | Applique aux vues 3D les tables des entités `color_correction` et `color_correction_volume` selon la distance de la caméra. Voir [Correction des couleurs](../features/color-correction.md). |
| `unprop_detail` | `true` | **Convertir le prop en brushes...** range les brushes dans un `func_detail`. |
| `unprop_keep_prop` | `false` | Garde le prop. |
| `unprop_shape` | `"auto"` | `auto`, `collision` ou `render`. |
| `model_lod.enabled` | `true` | Crée des niveaux de détail pour les modèles générés. |
| `model_lod.levels` | `4` | Niveaux sous le modèle complet, de 3 à 6. |
| `model_lod.reduction` | `0.5` | Part des triangles gardée à chaque niveau, de 0,2 à 0,9. |
| `propper_physics` | `false` | Crée un prop physique mobile depuis des brushes. |
| `propper_static` | `true` | Compile `$staticprop` et crée un `prop_static` ; décoché, crée un `prop_dynamic`. Désactivé pour les props physiques. |
| `propper_collision` | `true` | Génère la collision, obligatoire pour les props physiques. |
| `propper_mass` | `10.0` | Masse physique en kg, de 0,1 à 50000. |
| `combine_origin` | `"center"` | Origine d'un modèle fait par **Combiner des props** : `center` (centre) ou `bottom` (centre du bas) de la sélection. |
| `combine_keep_originals` | `true` | **Combiner des props** garde les props d'origine dans le visgroup caché `Combined props (originals)` ; désactivé : ils sont supprimés. |
| `combine_collision` | `true` | **Combiner des props** construit la collision à partir des morceaux .phy des props. |
| `combine_max_pieces` | `32` | Nombre maximal de morceaux de collision convexes d'un modèle combiné, de 1 à 128. |
| `model_keep_params` | `true` | L'import de modèles écrit la normal map, le brillant, l'auto-illumination et le tiling des textures du fichier dans les VMT. |

## `[sniffer]`

Le panneau du [BSP Sniffer](../features/bsp-sniffer.md) (Fichier > Importer > BSP Sniffer...). Seules les clés qui diffèrent du défaut sont écrites.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `address` | `""` | Serveur sous la forme `ip:port`. |
| `authorized` | `false` | Vous confirmez avoir le droit de tester le serveur (nécessaire pour démarrer). |
| `consent_seen` | `false` | L'avertissement a été accepté. |
| `output_dir` | `""` | Dossier de sortie (vide : le dossier de cache `sniffer`). |
| `file_name_template` | `"{map}_sniffed"` | Nom du fichier ; jetons `{map}` `{ip}` `{port}` `{date}` `{time}` `{datetime}`. |
| `gmod_dir` | `""` | Dossier de Garry's Mod (vide : automatique). |
| `launch_options` | fenêtré, 800 x 450, réglages bas, muet | Ajoutées au lancement Steam. |
| `insecure` | `"auto"` | `auto`, `always` ou `never` pour `-insecure`. |
| `game_bits` | `"auto"` | `auto`, `64` ou `32` : quel jeu de l'installation est lancé et lu. |
| `use_map_file` | `false` | Lire aussi le fichier de map du client pour recouper la mémoire et compléter ce qui lui manque. Désactivé : aucun fichier de map n'est ouvert. |
| `read_ldr_lighting` | `true` | Lire l'autre jeu d'éclairage avec un second chargement de la map en LDR. |
| `attach_running` | `false` | Utiliser un jeu déjà lancé. |
| `process_timeout_secs` | `180` | Attente du processus du jeu. |
| `load_timeout_secs` | `900` | Attente de la map. |
| `settle_secs` | `6` | La map doit rester identique ce temps. |
| `poll_secs` | `3` | Intervalle de lecture. |
| `save_models`, `save_materials`, `save_sounds`, `save_other` | `true` | Quel contenu enregistrer. |
| `only_non_stock` | `true` | Ignorer les fichiers du jeu de base. |
| `keep_bsp` | `true` | Garder le BSP récupéré. |
| `kill_game` | `true` | Fermer le jeu lancé. |
| `auto_disconnect` | `true` | Se déconnecter si le jeu reste ouvert. |
| `open_when_done` | `true` | Ouvrir le VMF à la fin. |
| `decompile.*` | `true` | `brushes`, `displacements`, `entities`, `brush_entities`, `static_props`, `overlays`, `cubemaps`, `areaportals`, `occluders`, `group_details`. |

## `[grid]`

**Map > Grilles**, le menu de grille de la barre d'outils et le gestionnaire de grilles. Voir [Grilles](../features/grids.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `mode` | `"Global"` | `Global`, `PerView`, `Custom`, `Selection`. |
| `custom` | `""` | Nom de la grille personnalisée du mode Custom. |
| `per_view` | 16 partout | `perspective`, `top`, `front`, `side` : chacune avec `size`, `origin`, `major_every`, `angle` (degrés, rotation de la grille dans cette vue). |
| `presets` | `[]` | Grilles personnalisées proposées dans toutes les maps (64 au plus) : `name`, `step` (par axe), `origin`, `angles`, `major_every`, `bounds` et `color` facultatifs. |
| `object_grids` | `true` | Modifier un objet qui porte une grille s'aligne sur cette grille. |
| `plane_3d` | `true` | Dessine le plan d'une grille personnalisée ou par vue en 3D. |

## `[mcp]` et `[discord]`

| Clé | Défaut | Effet |
| --- | --- | --- |
| `mcp.start_with_app` | `false` | Active le serveur MCP au démarrage de Hammer Pro. **Paramètres > MCP**. Voir [Serveur MCP](mcp-server.md). |
| `discord.enabled` | `true` | Affiche l'activité sur Discord. |
| `discord.show_map` | `true` | Affiche le nom de la map. |
| `discord.show_elapsed` | `true` | Affiche le temps passé. |
| `discord.show_tool` | `true` | Affiche l'outil actif. |
| `discord.application_id` | `""` | L'id de votre propre application Discord, de 17 à 20 chiffres ; vide : l'application de Hammer Pro. |
| `discord.project_url` | `""` | Lien du bouton (pas de bouton si vide). |

Voir [Discord Rich Presence](../features/discord.md).

## `[scan]`

| Clé | Défaut | Effet |
| --- | --- | --- |
| `scan.use_index` | `true` | Garde l'index du contenu enregistré (`content_index.bin` dans le dossier de cache) pour que les lancements n'analysent que les dossiers modifiés. **Paramètres > Jeu > Analyse du contenu**. |
| `scan.mode` | `"auto"` | `"auto"` (un dossier à la fois sur disque dur, en parallèle sur SSD et NVMe, par volume), `"parallel"` ou `"sequential"`. Appliqué au prochain chargement du contenu. |

Voir [Index et analyse du contenu](../features/content-index.md).

## `[update]`

| Clé | Défaut | Effet |
| --- | --- | --- |
| `update.check_on_startup` | `true` | Cherche une version plus récente en arrière-plan au démarrage de Hammer Pro. **Paramètres > Mises à jour**. |
| `update.show_dialog` | `true` | Affiche la fenêtre de mise à jour quand une version plus récente est trouvée. La réactiver efface aussi `update.skipped_version`. **Paramètres > Mises à jour**. |
| `update.skipped_version` | `""` | Tag de la version dont l'utilisateur ne veut plus entendre parler (**Ne plus afficher**), par exemple `"v1.0.3"`. Vide : aucune. |

Voir [Mises à jour](../features/updates.md).

## `[blender]`

**Paramètres > Lien avec Blender.** Voir [Lien avec Blender](../features/blender.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `blender.path` | `""` | Le programme Blender (`blender.exe`) ; vide veut dire que Hammer Pro le cherche. MCP le change avec `blender_setup`, qui le vérifie. |
| `blender.format` | `"gltf"` | Format de fichier d'un envoi : `gltf`, `obj` ou `fbx`. |
| `blender.live_link` | `true` | Surveille le fichier exporté et met la map à jour quand Blender l'écrit. |

## `[vtf]`

**Paramètres > Éditeur de VTF.** Voir [Éditeur de VTF](../features/vtf-editor.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `vtf.editor_path` | `""` | L'éditeur VTF externe (VTFEdit, VTFEdit++, VTFEdit Reloaded...) ; vide veut dire que Hammer Pro le cherche. Seul l'utilisateur le change. |
| `vtf.prefer_integrated` | `false` | Ouvre les textures dans l'éditeur VTF intégré même si un éditeur externe est trouvé. |

## `[input]`

**Paramètres > Saisie.** Voir [Pavé tactile](../features/trackpad.md).

| Clé | Défaut | Effet |
| --- | --- | --- |
| `input.trackpad_mode` | `"Auto"` | `Auto` active les gestes du pavé tactile quand un geste de pavé est vu et les désactive au premier cran de molette, `On` toujours, `Off` jamais (le comportement souris). |
| `input.trackpad_scroll_3d` | `"Orbit"` | Défilement à deux doigts dans la vue 3D : `Orbit` autour du point au centre de la vue, ou `Move` pour déplacer la caméra sur les côtés et vers le haut ou le bas. |
| `input.trackpad_pan_speed` | `1.0` | 0,25 à 4 : multiplie le déplacement au pavé tactile. |
| `input.trackpad_zoom_speed` | `1.0` | 0,25 à 4 : multiplie le zoom du pincement et de Ctrl + défilement. |
| `input.trackpad_orbit_speed` | `1.0` | 0,25 à 4 : multiplie l'orbite 3D. |

## Pages liées

- [Fenêtre des paramètres](../features/settings-dialog.md)
- [Fichiers et dossiers](files-and-folders.md)
- [Actions et raccourcis clavier](actions-shortcuts.md)

---
[Retour à l'index de la documentation](../README.md)
