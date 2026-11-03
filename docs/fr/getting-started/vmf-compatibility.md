[Documentation](../README.md) > Prise en main > Compatibilité VMF | [English](../../en/getting-started/vmf-compatibility.md)

# Compatibilité VMF

Hammer Pro ouvre et enregistre des fichiers `.vmf` standards : vous pouvez l'utiliser en parallèle de Hammer et de Hammer++ sur les mêmes maps. Cette page couvre l'ouverture, l'enregistrement, l'export et ce qui est conservé dans le fichier.

## Ouvrir des maps

| Comment | Effet |
|---|---|
| **Fichier > Ouvrir une map...** (`Ctrl+O`) | Ouvre un fichier `.vmf`, `.vmx` ou `.vmm`. Une map de style Quake `.map` (Valve 220) peut aussi être importée : elle s'ouvre comme un nouveau document, utilisez **Enregistrer sous** pour la garder en VMF. |
| Glisser-déposer | Déposez un `.vmf`, `.vmx` ou `.vmm` sur la fenêtre pour l'ouvrir. Les autres fichiers déposés sont aussi pris en charge : les pointfiles (`.lin`, `.pts`) se chargent, les fichiers de portails (`.prt`) et les archives (`.bsp`, `.gma`, `.vpk`) s'ouvrent dans leur visionneuse, les images ouvrent l'import de textures et les modèles (`.fbx`, `.obj`, `.gltf`, `.glb`) l'import de modèles. |
| **Fichier > Maps récentes** | Les 12 dernières maps, numérotées. **Vider la liste des maps récentes** l'efface (aussi dans **Paramètres > Général**). |
| Ligne de commande | `hammerpro chemin\vers\map.vmf`. |
| [Palette de commandes](../features/command-palette.md) | `Ctrl+P`, puis tapez le nom d'une map récente. |

Les maps s'ouvrent en arrière-plan : la map actuelle reste utilisable et la barre d'état affiche « Ouverture de <nom>... N s » avec un bouton pour arrêter. Chaque map ouverte a son propre onglet ; une map déjà ouverte est affichée au lieu d'être chargée deux fois (voir [Plusieurs maps ouvertes](../features/multiple-maps.md)). Si **Paramètres > Général > Ouvrir les maps dans de nouveaux onglets** est désactivé, une map ouverte remplace la map actuelle, après avoir proposé d'enregistrer ses modifications.

Quand une sauvegarde automatique plus récente de la map existe, Hammer Pro demande **Récupérer la sauvegarde automatique ?** avec **Ouvrir la sauvegarde automatique** ou **Garder la map enregistrée** (voir [Sauvegarde automatique et récupération](../features/autosave-crash.md)).

### Fichiers endommagés

La lecture est tolérante. Les clés sans valeur et les accolades fermantes en trop sont ignorées, les objets sans identifiant valide en reçoivent un nouveau, et les valeurs illisibles sont conservées telles quelles ; un toast indique « <nom> ouvert avec N problèmes » et la console les détaille. Un fichier tronqué (blocs non fermés, chaîne coupée au milieu) s'ouvre comme un nouveau document nommé « <nom> (endommagé) », pour que l'enregistrement ne puisse pas écraser le fichier abîmé : utilisez **Enregistrer sous** pour le garder sous un autre nom.

## Enregistrer

| Commande | Effet |
|---|---|
| **Fichier > Enregistrer** (`Ctrl+S`) | Enregistre la map. L'enregistrement est atomique (fichier temporaire, puis renommage) et, comme dans Hammer, l'ancien fichier est d'abord copié en `<nom>.vmx`. |
| **Fichier > Enregistrer sous...** (`Ctrl+Maj+S`) | Enregistre sous un nouveau nom. La fenêtre s'ouvre dans le **Dossier des maps** de **Paramètres > Jeu** s'il est réglé. |
| **Fichier > Enregistrer comme nouvelle version...** | Demande un nouveau nom de map (proposé : `gm_x` donne `gm_x_v2`, `gm_x_v2` donne `gm_x_v3` ; minuscules, chiffres, `_` et `-` uniquement), y enregistre la map, puis copie sous le nouveau nom la map compilée (avec ses chemins générés renommés, pour que les cubemaps construites restent valables), son nav mesh et ses scripts propres à la map (particules, sons de niveau, soundscapes). Les fichiers existants ne sont jamais remplacés. Une map jamais enregistrée passe par **Enregistrer sous**. |

Comme dans Hammer, l'enregistrement écrit dans la map la caméra 3D actuelle et les réglages de grille (`nGridSpacing`, `bSnapToGrid`, `bShowGrid` de `viewsettings`).

## Aller-retour sans perte

- Tout ce que Hammer Pro ne comprend pas (clés inconnues, blocs inconnus, données illisibles) est conservé et réécrit tel quel. Ouvrir puis enregistrer un fichier ne supprime jamais de données en silence.
- Le format du texte trouvé à l'ouverture est conservé : texte Windows-1252, fins de ligne CRLF et écriture des nombres de Hammer. Une map écrite par Hammer et enregistrée sans modification ressort telle que Hammer l'a écrite.
- Les identifiants, groupes, visgroups, cordons, caméras, objets masqués et connexions des entités sont conservés comme dans Hammer.

### Données de Hammer++

Les blocs ajoutés par Hammer++ sont conservés : les sommets exacts des faces de `vertices_plus` dans chaque face, et les blocs `*_plus` au niveau de la map (`palette_plus`, `colorcorrection_plus`, `light_plus`, `bgimages_plus` et les autres). Une map peut faire des allers-retours entre Hammer++ et Hammer Pro.

## Le fichier annexe (sidecar)

Les données d'édition qui n'ont pas leur place dans le VMF sont rangées à part, dans un petit fichier TOML par map : `%LOCALAPPDATA%\HammerPro\maps\<clé de la map>.toml` (la clé identifie le chemin complet de la map). Il contient des données de confort : état des vues, problèmes ignorés, empreinte de la dernière compilation, jeu cible de la map, dernier chemin d'export, identifiant Workshop. Il est écrit à l'enregistrement et à la fermeture de la map. Un fichier annexe absent ou abîmé est simplement lu comme les valeurs par défaut : ce n'est jamais la seule copie de votre travail, et le VMF reste un VMF ordinaire pour Hammer. Voir [Fichiers et dossiers](../reference/files-and-folders.md).

## Importer et exporter

| Commande | Effet |
|---|---|
| **Fichier > Importer une map dans la map actuelle...** | Fusionne un autre VMF dans la map ouverte, en une seule étape d'annulation. Les groupes, visgroups et objets masqués sont conservés. |
| **Fichier > Exporter la sélection en VMF...** | Écrit les objets sélectionnés dans un nouveau VMF (nom proposé `prefab.vmf`), avec les groupes auxquels ils appartiennent et leurs visgroups. |
| **Fichier > Exporter une copie...** | Écrit toute la map dans un autre VMF sans renommer le document ouvert. Le chemin est mémorisé avec la map. |
| **Fichier > Exporter à nouveau** (`Alt+B`) | Réécrit la copie au dernier chemin d'export de cette map, sans rien demander. |
| **Fichier > Exporter en OBJ...** | La géométrie des brushes et des displacements avec les coordonnées de texture, pour les logiciels 3D (voir [Outils BSP](../features/bsp-tools.md)). |

Les exports sont écrits par une tâche de fond, de façon atomique.

## FAQ

**Puis-je rouvrir ma map dans Hammer après l'avoir modifiée dans Hammer Pro ?** Oui. Le fichier est un VMF standard. La plupart des données propres à Hammer Pro sont dans le fichier annexe. Seule exception : les grilles personnalisées de la map, écrites dans un bloc `hammerpro_grids` du VMF (voir [Grilles](../features/grids.md)).

**Hammer++ perd-il ses données si j'enregistre dans Hammer Pro ?** Non. `vertices_plus` et les blocs `*_plus` sont réécrits.

**À quoi sert le fichier `.vmx` à côté de ma map ?** C'est la version précédente de la map, copiée à chaque enregistrement comme le fait Hammer. Ouvrez-le avec **Fichier > Ouvrir une map...** pour revenir en arrière.

**Hammer Pro peut-il ouvrir un `.bsp` compilé ?** Oui, avec **Fichier > Décompilateur de BSP...**, qui le décompile en une nouvelle map (voir [Outils BSP](../features/bsp-tools.md)).

## Pages liées

- [Plusieurs maps ouvertes](../features/multiple-maps.md)
- [Sauvegarde automatique et récupération](../features/autosave-crash.md)
- [Presse-papiers](../features/clipboard.md)
- [Outils BSP](../features/bsp-tools.md)
- [Fichiers et dossiers](../reference/files-and-folders.md)

---
[Retour à l'index de la documentation](../README.md)
