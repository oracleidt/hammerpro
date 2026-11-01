[Documentation](../README.md) > Référence > Actions et raccourcis clavier | [English](../../en/reference/actions-shortcuts.md)

# Actions et raccourcis clavier

Chaque commande de Hammer Pro 1.0.1 est une *action* : une seule entrée partagée par les menus, la barre d'outils, les raccourcis clavier, la palette de commandes (`Ctrl+P`), les fichiers `--script` et le serveur MCP (`run_action`, `list_actions`). Cette page liste les 261 actions, menu par menu, dans l'ordre où les menus les affichent.

Colonnes :

- **Commande** : le libellé de l'interface en français (précédé de son sous-menu s'il en a un).
- **Id** : l'identifiant stable, utilisé par `[keybindings]` dans `settings.toml`, par les scripts et par l'outil MCP `run_action`.
- **Par défaut** : le raccourci du jeu de raccourcis **Hammer Pro** (celui par défaut).
- **Hammer** / **Hammer++** : le raccourci quand ce jeu est choisi dans **Paramètres > Clavier > Jeu de raccourcis**, seulement s'il diffère de celui de Hammer Pro ("(aucun)" : pas de raccourci dans ce jeu).

Avec le jeu Hammer Pro, deux touches de Hammer marchent aussi en raccourcis supplémentaires, car elles n'entrent en conflit avec rien : `Maj+Q` (**Effacer la sélection**) et `Maj+R` (**Afficher la grille**). Les raccourcis que vous modifiez vous-même passent toujours avant le jeu choisi. Voir [Raccourcis clavier](../features/keyboard-shortcuts.md) pour changer un raccourci, les conflits et la fenêtre de référence `F1`.

Les touches qui ne sont pas des actions (voler dans la vue 3D avec `Z Q S D`, les gestes de la souris, les touches d'un outil actif, celles de la vue joueur) sont décrites sur les pages des outils : [Caméra et navigation](../features/camera-navigation.md), [Outil de sélection](../features/selection-tool.md), [Vue joueur](../features/player-view.md).


## Menu Fichier

| Commande | Id | Par défaut | Hammer | Hammer++ |
|---|---|---|---|---|
| Nouvelle map | `new_map` | `Ctrl+N` |  |  |
| Ouvrir une map... | `open_map` | `Ctrl+O` |  |  |
| Fermer | `close_map` | `Ctrl+F4` |  |  |
| Fermer les autres maps | `close_other_maps` |  |  |  |
| Enregistrer | `save_map` | `Ctrl+S` |  |  |
| Enregistrer sous... | `save_map_as` | `Ctrl+Maj+S` |  |  |
| Enregistrer comme nouvelle version... | `save_as_new_version` |  |  |  |
| Importer une map dans la map actuelle... | `import_map` |  |  |  |
| Exporter la sélection en VMF... | `export_selection` |  |  |  |
| Exporter une copie... | `export_copy` |  |  |  |
| Exporter à nouveau | `export_again` | `Alt+B` |  |  |
| Exporter en OBJ... | `export_obj` |  |  |  |
| Importer les props depuis le jeu | `import_from_game` |  |  |  |
| Ouvrir une archive... | `open_archive` |  |  |  |
| Ouvrir une map compilée (BSP)... | `open_bsp` |  |  |  |
| Importer un modèle (FBX, OBJ, glTF)... | `import_model` |  |  |  |
| Compiler et lancer... | `run_map` | `Ctrl+F9` | `F9` | `F9` |
| Compiler maintenant | `compile_now` | `F9` | `Ctrl+F9` | `Ctrl+F9` |
| Annuler la compilation | `cancel_compile` |  |  |  |
| Compilation par lots... | `batch_compile` |  |  |  |
| Lancer la map dans le jeu | `launch_game` |  |  |  |
| Recharger la map dans le jeu | `reload_in_game` |  |  |  |
| Construire et enregistrer en contenu... | `build_content` |  |  |  |
| Publier sur le Workshop... | `publish_workshop` |  |  |  |
| Quitter | `exit` |  |  |  |

## Menu Édition

| Commande | Id | Par défaut | Hammer | Hammer++ |
|---|---|---|---|---|
| Annuler | `undo` | `Ctrl+Z` |  |  |
| Rétablir | `redo` | `Ctrl+Y` |  |  |
| Rechercher des entités... | `find_entities` | `Ctrl+Maj+F` |  |  |
| Rechercher et remplacer dans les entités... | `find_replace_text` | `Ctrl+Alt+R` | `Ctrl+Maj+R` | `Ctrl+Maj+R` |
| Couper | `cut` | `Ctrl+X` |  |  |
| Copier | `copy` | `Ctrl+C` |  |  |
| Coller | `paste` | `Ctrl+V` |  |  |
| Collage spécial... | `paste_special` | `Ctrl+Maj+V` |  |  |
| Supprimer | `delete` | `Suppr` |  |  |
| Dupliquer | `duplicate` | `Ctrl+D` |  |  |
| Copier les keyvalues | `copy_keyvalues` |  |  |  |
| Coller les keyvalues | `paste_keyvalues` |  |  |  |
| Tout sélectionner | `select_all` | `Ctrl+A` | `Ctrl+Maj+A` | `Ctrl+Maj+A` |
| Effacer la sélection | `select_none` | `Ctrl+Maj+A` | `Maj+Q` | `Maj+Q` |
| Inverser la sélection | `invert_selection` | `Ctrl+I` | `Ctrl+Maj+I` | `Ctrl+Maj+I` |
| Sélection de faces précédente | `face_selection_back` | `Alt+Z` |  |  |
| Sélection de faces suivante | `face_selection_forward` | `Alt+Y` |  |  |
| Mode de sélection > Groupes | `select_mode_groups` |  |  |  |
| Mode de sélection > Objets | `select_mode_objects` |  |  |  |
| Mode de sélection > Solides | `select_mode_solids` |  |  |  |
| Mode de sélection > Ignorer les groupes | `toggle_ignore_groups` | `Ctrl+W` |  |  |
| Propriétés | `properties` | `Alt+Entrée` |  |  |
| Modifier la couleur... | `edit_color` | `C` |  |  |
| Définir la distance maximale... | `set_max_distance` |  |  |  |

## Menu Map

| Commande | Id | Par défaut | Hammer | Hammer++ |
|---|---|---|---|---|
| Grille plus fine | `grid_smaller` | `[` |  |  |
| Grille plus large | `grid_larger` | `]` |  |  |
| Afficher la grille | `toggle_grid` | `Maj+G` | `Maj+R` | `Maj+R` |
| Magnétisme de grille | `toggle_snap` | `Maj+W` |  |  |
| Grille 3D sur les faces | `toggle_grid_3d` | `P` |  |  |
| Grilles > Grille globale | `grid_mode_global` |  |  |  |
| Grilles > Une grille par vue | `grid_mode_per_view` |  |  |  |
| Grilles > Grille personnalisée | `grid_mode_custom` |  |  |  |
| Grilles > Grille de la sélection | `grid_mode_selection` |  |  |  |
| Grilles > Grille depuis la sélection | `grid_from_selection` |  |  |  |
| Grilles > Grille depuis la face | `grid_from_face` |  |  |  |
| Grilles > Attacher la grille à la sélection | `attach_grid` |  |  |  |
| Grilles > Détacher la grille de la sélection | `detach_grid` |  |  |  |
| Grilles > Gestionnaire de grilles... | `grid_manager` |  |  |  |
| Vérifier les problèmes | `check_problems` | `Alt+P` |  |  |
| Rapport des entités | `entity_report` |  |  |  |
| Aller à un id de brush ou d'entité... | `go_to_brush` | `Ctrl+Maj+G` |  |  |
| Soundscapes... | `edit_soundscapes` |  |  |  |
| Ressources de la map | `show_map_assets` |  |  |  |
| Vérifier les fuites | `check_leaks` |  |  |  |
| Fermer l'ouverture par un areaportal | `seal_areaportal` |  |  |  |
| Suggestions de func_detail... | `show_detail_suggestions` |  |  |  |
| Comparer avec une autre map... | `compare_maps` |  |  |  |
| Insérer la galerie d'entités | `entity_gallery` |  |  |  |
| Charger le pointfile de fuite... | `load_pointfile` |  |  |  |
| Décharger le pointfile | `unload_pointfile` |  |  |  |
| Charger le fichier de portails... | `load_portal_file` |  |  |  |
| Décharger le fichier de portails | `unload_portal_file` |  |  |  |
| Portails et visibilité... | `show_portal_viewer` |  |  |  |
| Construire les portails (géométrie seule) | `build_portals` | `Ctrl+Maj+P` |  |  |
| Charger un nav mesh... | `load_nav_mesh` |  |  |  |
| Propriétés de la map... | `map_properties` |  |  |  |
| Informations sur la map | `map_info` |  |  |  |
| Éclairage > Reconstruire les cubemaps | `rebuild_cubemaps` |  |  |  |
| Éclairage > Écrire les cubemaps dans la map compilée | `write_cubemaps` |  |  |  |
| Éclairage > Précalculer l'éclairage | `bake_lighting` |  |  |  |
| Éclairage > Précalculer autour de la caméra | `bake_around_camera` |  |  |  |
| Éclairage > Arrêter le précalcul | `stop_bake` |  |  |  |
| Éclairage > Sélectionner les faces à petite échelle de lightmap | `select_tiny_lightmap_faces` |  |  |  |

## Menu Affichage

| Commande | Id | Par défaut | Hammer | Hammer++ |
|---|---|---|---|---|
| Vue de dessus | `view_top` | `F2` |  |  |
| Vue de côté | `view_side` | `F3` |  |  |
| Vue de face | `view_front` | `F4` |  |  |
| Vue 3D texturée | `view_3d_textured` | `F5` |  |  |
| Vue 3D unie | `view_3d_flat` | `Maj+F5` |  |  |
| Vue 2D suivante | `cycle_view_2d` | `Tab` |  |  |
| Vue logique 2D | `view_logical` |  |  |  |
| Vue logique | `show_logical_view` |  |  |  |
| Mode de rendu 3D > Texturé | `render_textured` | `Alt+1` |  |  |
| Mode de rendu 3D > Couleurs unies | `render_flat` | `Alt+2` |  |  |
| Mode de rendu 3D > Filaire | `render_wireframe` | `Alt+3` |  |  |
| Mode de rendu 3D > Texturé + arêtes du mesh | `render_textured_wire` | `Alt+4` |  |  |
| Mode de rendu 3D > Aperçu de l'éclairage | `render_lit` | `Alt+5` |  |  |
| Mode de rendu 3D > Texturé sans ombrage | `render_unshaded` | `Alt+6` |  |  |
| Mode de rendu 3D > Groupes de lissage | `render_smoothing_groups` | `Alt+7` |  |  |
| Mode de rendu 3D > Grille de lightmap | `render_lightmap_grid` | `Alt+8` |  |  |
| Mode de rendu 3D > Aperçu de l'éclairage précalculé | `render_baked` | `Ctrl+F5` |  |  |
| Afficher les modèles | `toggle_models` |  |  |  |
| Afficher les aides et sprites | `toggle_helpers` |  |  |  |
| Afficher les textures outils | `toggle_tool_textures` |  |  | `Ctrl+Maj+F2` |
| Images de fond... | `background_images` |  |  |  |
| Options d'affichage > Informations de la vue 3D | `toggle_view_info` | `O` |  |  |
| Options d'affichage > Boîtes englobantes | `toggle_bounds_overlay` | `I` |  |  |
| Options d'affichage > Afficher les overlays | `toggle_overlays` |  |  |  |
| Options d'affichage > Afficher les connexions | `toggle_connections` |  |  |  |
| Options d'affichage > Afficher les objets d'éditeur | `toggle_editor_objects` |  |  | `Alt+O` |
| Options d'affichage > Afficher les entités ponctuelles | `toggle_point_entities` |  |  |  |
| Options d'affichage > Afficher les noms des entités | `toggle_entity_names` |  |  |  |
| Options d'affichage > Aides de toutes les entités | `toggle_helpers_all` |  |  |  |
| Options d'affichage > Afficher les cordes | `toggle_ropes` |  |  |  |
| Options d'affichage > Simuler les cordes | `simulate_ropes` |  |  |  |
| Options d'affichage > Effets de rendu | `toggle_render_effects` |  |  |  |
| Options d'affichage > Aperçu du ciel | `toggle_sky_preview` |  |  |  |
| Options d'affichage > Skybox 3D | `toggle_skybox_3d` | `Alt+V` |  |  |
| Options d'affichage > Modèles de collision | `toggle_collision_models` |  |  |  |
| Options d'affichage > Aperçu du brouillard | `toggle_fog_preview` |  |  |  |
| Options d'affichage > Matériaux animés | `toggle_animated_materials` |  |  |  |
| Options d'affichage > Fondu des modèles | `toggle_model_fade` |  |  |  |
| Options d'affichage > Matériaux opaques | `toggle_opaque_materials` |  |  |  |
| Options d'affichage > Pentes praticables | `toggle_walkable` |  |  |  |
| Options d'affichage > Afficher les displacements | `toggle_displacements` |  |  |  |
| Options d'affichage > Limite de distance circulaire | `toggle_radius_culling` |  |  |  |
| Options d'affichage > Surligner les brushes invalides | `toggle_invalid_brushes` |  |  |  |
| Options d'affichage > Utiliser en jeu la sky_camera affichée | `use_sky_camera_in_game` |  |  |  |
| Options d'affichage > Maillage des displacements en vue 2D | `toggle_disp_mesh_2d` |  |  |  |
| Options d'affichage > Praticabilité des displacements | `toggle_disp_walkable` |  |  |  |
| Options d'affichage > Afficher le nodraw | `toggle_nodraw` |  |  |  |
| Options d'affichage > Afficher les collisions des modèles | `toggle_collision_only` |  |  |  |
| Options d'affichage > Aperçu de l'eau | `toggle_water_preview` |  |  |  |
| Options d'affichage > Aperçu des fog_volume | `toggle_fog_volumes` |  |  |  |
| Options d'affichage > Texture des fog_volume | `toggle_fog_volume_texture` |  |  |  |
| Options d'affichage > Aperçu des poissons | `toggle_fish_preview` |  |  |  |
| Options d'affichage > Poissons qui nagent | `toggle_fish_swim` |  |  |  |
| Options d'affichage > Reflets des cubemaps | `toggle_cubemaps` |  |  |  |
| Options d'affichage > Afficher les clusters visibles | `toggle_pvs` |  |  |  |
| Options d'affichage > Afficher le nav mesh | `toggle_nav_mesh` |  |  |  |
| Options d'affichage > Afficher les particules | `toggle_particles` |  |  |  |
| Options d'affichage > Particules de la sélection uniquement | `toggle_particles_selected` |  |  |  |
| Options d'affichage > Vue joueur | `toggle_player_view` | `F6` |  |  |
| Masquer la sélection | `hide_selected` | `H` |  |  |
| Masquer le reste | `hide_unselected` | `Ctrl+Maj+H` |  |  |
| Tout afficher | `unhide_all` | `U` |  |  |
| Utiliser les cordons | `toggle_cordon` |  |  |  |
| Déplacer vers un visgroup... | `move_to_visgroup` |  |  |  |
| Créer un visgroup des objets masqués | `hidden_to_visgroup` |  |  |  |
| Centrer les vues 2D sur la sélection | `center_2d` | `Ctrl+E` |  |  |
| Centrer la vue 3D sur la sélection | `center_3d` | `Ctrl+Maj+E` |  |  |
| Agrandir la vue | `maximize_view` | `Maj+Z` |  |  |
| Zoom avant des vues 2D | `zoom_2d_in` | `+` |  |  |
| Zoom arrière des vues 2D | `zoom_2d_out` | `-` |  |  |
| Réinitialiser le partage des vues | `reset_view_splits` |  | `Ctrl+A` | `Ctrl+A` |
| Distance d'affichage plus courte | `far_plane_closer` |  |  |  |
| Distance d'affichage plus longue | `far_plane_farther` |  |  |  |
| Panneaux > Navigateur de textures | `show_texture_browser` |  |  |  |
| Panneaux > Navigateur de modèles | `show_model_browser` |  |  | `Maj+F10` |
| Panneaux > Propriétés de l'entité | `show_entity_properties` |  |  |  |
| Panneaux > Visgroups | `show_visgroups` |  |  |  |
| Panneaux > Arborescence | `show_outliner` |  |  |  |
| Panneaux > Historique des modifications | `show_history` |  |  |  |
| Panneaux > Barre d'outils | `show_toolbar` |  |  |  |
| Panneaux > Palette d'outils | `show_tool_palette` |  |  |  |
| Panneaux > Barre d'état | `show_status_bar` |  |  |  |
| Panneaux > Navigateur de sons | `show_sound_browser` |  |  | `Maj+F12` |
| Panneaux > Navigateur de particules | `show_particle_browser` |  |  | `Maj+F11` |
| Panneaux > Éclairage | `show_lighting` |  |  |  |
| Panneaux > Prefabs | `show_prefabs` |  |  |  |

## Menu Outils

| Commande | Id | Par défaut | Hammer | Hammer++ |
|---|---|---|---|---|
| Rechercher et remplacer des textures... | `replace_textures` | `Ctrl+Maj+R` | (aucun) | (aucun) |
| Grouper | `group` | `Ctrl+G` |  |  |
| Dégrouper | `ungroup` | `Ctrl+U` |  |  |
| Lier à une entité | `tie_to_entity` | `Ctrl+T` |  |  |
| Renvoyer au monde | `move_to_world` | `Ctrl+Maj+W` |  |  |
| Creuser (carve) | `carve` | `Ctrl+Maj+C` |  |  |
| Rendre creux | `make_hollow` | `Ctrl+H` |  |  |
| Fusionner en un solide convexe | `merge_convex` |  |  |  |
| Aligner la sélection sur la grille | `snap_to_grid` | `Ctrl+B` |  |  |
| Transformer... | `transform_dialog` | `Ctrl+M` |  |  |
| Verrouillage des textures | `toggle_texture_lock` | `Maj+L` |  |  |
| Verrouillage de l'échelle des textures | `toggle_texture_scale_lock` | `Maj+K` |  |  |
| Poser au sol | `move_to_floor` | `Alt+N` |  |  |
| Coller au plafond | `move_to_ceiling` | `Alt+U` |  |  |
| Aligner chaque objet sur la grille | `snap_to_grid_individually` | `Ctrl+Maj+B` |  |  |
| Centrer les origines | `center_origins` |  |  |  |
| Rotation Z aléatoire | `random_rotate` |  |  |  |
| Options de rotation Z aléatoire... | `random_rotate_dialog` |  |  |  |
| Pivot déplaçable | `toggle_pivot` |  |  |  |
| Poser au sol, aligné sur la surface | `move_to_floor_aligned` | `Ctrl+Alt+N` |  |  |
| Coller au plafond, aligné sur la surface | `move_to_ceiling_aligned` | `Ctrl+Alt+U` |  |  |
| Fusionner les brushes | `merge_brushes` | `Ctrl+Maj+M` |  |  |
| Nodraw sur les faces cachées... | `nodraw_hidden_faces` |  |  |  |
| Lier chacun à sa propre entité | `tie_each_to_entity` | `Ctrl+Maj+T` |  |  |
| Convertir en func_detail | `convert_to_detail` |  |  |  |
| Aligner > Aligner à gauche | `align_left` |  |  |  |
| Aligner > Aligner à droite | `align_right` |  |  |  |
| Aligner > Aligner en haut | `align_top` |  |  |  |
| Aligner > Aligner en bas | `align_bottom` |  |  |  |
| Aligner > Centrer horizontalement | `align_center_horizontal` |  |  |  |
| Aligner > Centrer verticalement | `align_center_vertical` |  |  |  |
| Retourner > Retourner X | `flip_x` |  |  |  |
| Retourner > Retourner Y | `flip_y` |  |  |  |
| Retourner > Retourner Z | `flip_z` |  |  |  |
| Retourner > Retourner horizontalement | `flip_horizontal` | `Ctrl+L` |  |  |
| Retourner > Retourner verticalement | `flip_vertical` |  | `Ctrl+I` | `Ctrl+I` |
| Insérer un point de chemin après la sélection | `path_insert_point` |  |  |  |
| Supprimer des points de chemin et relier | `path_remove_point` |  |  |  |
| Créer un prefab... | `create_prefab` | `Ctrl+R` |  |  |
| Insérer le prefab à sa taille d'origine | `insert_original_prefab` | `Alt+Maj+C` |  |  |
| Placement physique (chute et pose) | `physics_placement` |  |  |  |
| Convertir les brushes en prop statique | `brushes_to_prop` |  |  |  |
| Convertir le prop en brushes... | `prop_to_brushes` |  |  |  |
| Générer le nav mesh | `generate_nav_mesh` |  |  |  |
| Options des props statiques (Propper)... | `propper_options` |  |  |  |
| Instances > Créer une instance depuis la sélection... | `create_instance` | `Alt+S` |  |  |
| Instances > Modifier l'instance | `edit_instance` |  |  |  |
| Instances > Fusionner les instances sélectionnées | `collapse_instances` |  |  |  |
| Instances > Fusionner toutes les instances... | `collapse_all_instances` |  |  |  |
| Instances > Instances : masquées | `instances_hidden` |  |  |  |
| Instances > Instances : teintées | `instances_tinted` |  |  |  |
| Instances > Instances : normales | `instances_normal` |  |  |  |
| Outil de sélection | `tool_select` | `Maj+S` |  |  |
| Outil caméra | `tool_camera` | `Maj+C` |  |  |
| Outil entité | `tool_entity` | `Maj+E` |  |  |
| Outil bloc | `tool_block` | `Maj+B` |  |  |
| Application de texture | `tool_texture` | `Maj+A` |  |  |
| Appliquer la texture actuelle | `apply_current_texture` | `Maj+T` |  |  |
| Outil décal | `tool_decal` | `Maj+D` |  |  |
| Outil overlay | `tool_overlay` | `Maj+O` |  |  |
| Outil de découpe | `tool_clip` | `Maj+X` |  |  |
| Outil extruder / creuser | `tool_extrude` | `Maj+I` |  |  |
| Outil vertex | `tool_vertex` | `Maj+V` |  |  |
| Outil displacement | `tool_displacement` | `Maj+P` |  |  |
| Outil polygone | `tool_polygon` | `Maj+N` |  |  |
| Outil courbe | `tool_curve` | `Maj+U` |  |  |
| Outil cordon | `tool_cordon` |  |  |  |
| Outil chemin | `tool_path` |  |  |  |
| Outil sprinkle | `tool_sprinkle` |  |  |  |
| Scinder la face | `vertex_split_face` | `Ctrl+F` |  |  |
| Mettre les sommets à l'échelle... | `vertex_scale` | `Alt+E` |  |  |
| Subdiviser les displacements | `disp_subdivide` |  |  |  |
| Inverser l'alpha des displacements | `disp_invert_alpha` |  |  |  |
| Ajouter du bruit aux displacements... | `disp_noise` |  |  |  |
| Verrouillage UV (outil sommets) | `toggle_uv_lock` |  |  |  |
| Recharger le contenu depuis le disque | `reload_content` |  |  |  |
| Serveur MCP | `toggle_mcp_server` |  |  |  |
| Installer MCP dans Claude, Codex et Gemini | `mcp_install` |  |  |  |
| MCP... | `mcp_panel` |  |  |  |
| Palette de commandes... | `command_palette` | `Ctrl+P` |  |  |
| Options... | `settings` | `Ctrl+,` |  |  |

## Menu Fenêtre

| Commande | Id | Par défaut | Hammer | Hammer++ |
|---|---|---|---|---|
| Disposition > Enregistrer la disposition... | `save_layout` |  |  |  |
| Disposition > Réinitialiser la disposition | `reset_layout` |  |  |  |
| Console | `show_console` | `` ` `` |  |  |
| Map suivante | `next_map` | `Ctrl+Tab` |  |  |
| Map précédente | `previous_map` | `Ctrl+Maj+Tab` |  |  |

## Menu Aide

| Commande | Id | Par défaut | Hammer | Hammer++ |
|---|---|---|---|---|
| Raccourcis clavier | `keyboard_shortcuts` | `F1` |  |  |
| Documentation | `open_docs` |  |  |  |
| Signaler un problème | `report_issue` |  |  |  |
| Aide de l'entité | `entity_help` |  |  |  |
| À propos de Hammer Pro | `about` |  |  |  |

## Palette de commandes uniquement

| Commande | Id | Par défaut | Hammer | Hammer++ |
|---|---|---|---|---|
| Gizmo en repère local | `toggle_gizmo_local` | `Ctrl+J` |  |  |
| Créer un brush hint | `make_hint_brush` |  |  |  |

## Pages liées

- [Raccourcis clavier](../features/keyboard-shortcuts.md)
- [Palette de commandes](../features/command-palette.md)
- [Référence des paramètres](settings.md)
- [Commandes de script](script-commands.md)
- [Serveur MCP](mcp-server.md)

---
[Retour à l'index de la documentation](../README.md)
