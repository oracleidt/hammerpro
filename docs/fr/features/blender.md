[Documentation](../README.md) > Fonctionnalités > Lien avec Blender | [English](../../en/features/blender.md) | [Русский](../../ru/features/blender.md)

# Lien avec Blender

Le lien avec Blender est un aller-retour entre Hammer Pro et Blender. Son usage principal est celui des **displacements** : envoyez un ou plusieurs displacements (raccordés ou non) vers Blender, sculptez-les et retravaillez-les avec les outils de Blender, et le résultat revient dans les **mêmes displacements** avec la nouvelle forme, en gardant tout le reste de ce que contient le displacement. Il marche aussi pour les **brushes** (ils reviennent en brushes) et pour les **props** (le modèle est recompilé et tous les props qui l'utilisent sont rafraîchis).

## Le déroulement en bref

1. Dans Hammer Pro, sélectionnez les displacements (les faces avec l'[outil displacement](displacement-tool.md), ou les brushes qui les portent) et choisissez **Outils > Blender et MCP > Envoyer vers Blender**.
2. Blender démarre avec les displacements importés, un objet par displacement, texturés avec les matériaux de la map.
3. Sculptez ou modifiez les sommets dans Blender (mode sculpture, édition proportionnelle, modificateurs appliqués à la main...). Gardez les sommets et les faces de chaque objet : déplacez-les, n'en ajoutez pas et n'en supprimez pas.
4. Cliquez sur **Update in Hammer Pro** (menu **Hammer Pro** de la barre du haut, ou l'onglet de la barre latérale), ou enregistrez simplement le fichier `.blend`. Hammer Pro récupère le fichier en moins d'une seconde et réécrit les displacements. Une seule étape d'annulation : **Mise à jour depuis Blender**.
5. Continuez : renvoyez, ou répétez 3 et 4 autant que vous voulez.

## L'ouvrir

- **Outils > Blender et MCP > Envoyer vers Blender** (grisé tant que rien n'est sélectionné), et le bouton correspondant de la palette d'outils.
- **Paramètres > Lien avec Blender** : le programme, le format de fichier, le lien direct et les mêmes actions en boutons.
- **Outils > Blender et MCP > Mettre à jour depuis Blender** (grisé sans lien) : lit le fichier tout de suite. **Outils > Blender et MCP > Arrêter le lien direct avec Blender** et **Outils > Blender et MCP > Installer l'add-on Blender**.
- La palette de commandes propose les quatre. Pas de raccourci par défaut.

## Ce qui est envoyé

| Sélection | Ce qui est exporté | Ce qui revient |
|---|---|---|
| Displacements : faces de displacement sélectionnées, ou brushes et entités sélectionnés qui ont des displacements | Un objet maillage par displacement, nommé `HPDISP_<solide>_<face>_p<puissance>` (les ids du brush et de la face, et la puissance). Sa grille de sommets dans l'ordre et la triangulation exacts du displacement, avec les positions finales, les normales et les UV de la face. En glTF chaque objet porte aussi l'identité en propriétés personnalisées (`hammerpro` : kind, solid, side, power, material). Quand une sélection contient des displacements, seuls eux sont envoyés. | Les mêmes displacements, remodelés. |
| Brushes, entités brush, groupes (sans displacement) | Toutes les faces visibles avec leurs coordonnées de texture et normales. Les textures `tools/` sont exclues. | De nouveaux brushes à la place de ceux envoyés. |
| Un prop (`prop_*` avec un `.mdl`) | Le maillage de rendu de son modèle, dans les coordonnées propres du modèle. Tous les props sélectionnés avec le même modèle sont liés au fichier. | Le modèle recompilé sous `models/hammerpro`, puis chaque prop lié l'utilise. |

Les matériaux gardent le nom du matériau Source (par exemple `dev/dev_measuregeneric01`) et leur texture, en PNG de 512 pixels au plus (incluse dans le glTF, à côté du fichier en OBJ et FBX), donc les objets sont corrects dans Blender tout de suite.

1024 displacements au plus par envoi.

### Formats

| Format | Fichier | Remarques |
|---|---|---|
| **glTF** (par défaut) | `.glb`, un seul fichier | Textures incluses, identité en propriétés personnalisées. Le choix le plus fiable. |
| **OBJ** | `.obj`, `.mtl` et les textures PNG | Un objet `o` par displacement. |
| **FBX** | `.fbx` binaire avec les textures PNG à côté | Écrit en FBX 7.4, un modèle par displacement. |

Le choix est enregistré (`blender.format`, `gltf` par défaut). Les trois donnent le même résultat ; Blender 5.2 a servi à vérifier les trois allers-retours.

### Unités et axes

Les fichiers sont en mètres avec Y vers le haut, comme Blender l'attend, avec 1 mètre égal à 39,37 unités Source (la même unité que l'[import de modèles](model-import.md)). Blender en fait son propre Z vers le haut (glTF) ou garde les axes du fichier avec un objet pivoté (OBJ, FBX) : modifiez dans la vue comme d'habitude, l'export reconvertit. Ne changez pas l'échelle d'unité de la scène et n'appliquez ni ne réinitialisez les transformations des objets de displacement d'une façon qui déplace les sommets.

## Ce que fait la mise à jour aux displacements

Chaque objet du fichier est examiné par son nom :

- **Nommé `HPDISP_...` et grille intacte** (même nombre de triangles, sommets à leur place, coordonnées de texture inchangées) : les nouvelles positions des sommets sont écrites dans le displacement. Seuls les sommets que vous avez déplacés changent (un déplacement de moins de 0,02 unité environ compte pour aucun). L'alpha, les tags, les flags (collision), l'échelle de lightmap, la puissance, l'alignement de texture, les rangées de mélange (mélange à quatre) et les sommets autorisés restent tels quels. Un sommet déplacé garde son vecteur de direction : sa hauteur devient la distance le long de ce vecteur, et un déplacement latéral est stocké comme décalage du sommet. Les tags de triangle (praticable, constructible) sont recalculés d'après la nouvelle forme.
- **Nommé `HPDISP_...` mais remodelé** (vous avez subdivisé, supprimé, fusionné ou réordonné) : le nouveau maillage est projeté sur le displacement, comme [Plaquer sur un maillage](displacement-tool.md#plaquer-sur-un-maillage), selon la normale moyenne du displacement. L'alpha, les flags et les tags sont gardés, la forme suit le maillage.
- **Tout autre objet**, ou un nom qui n'a pas été envoyé : il devient de nouveaux displacements avec les dalles du **maillage en displacement** (dalles de 256 unités, puissance automatique, le matériau courant), à sa place.

Ensuite les raccords entre les displacements mis à jour sont **recousus**, comme **Coudre** dans l'outil displacement. Les displacements voisins des mis à jour que vous n'avez pas envoyés ne sont pas touchés : envoyez aussi les voisins quand un raccord doit suivre.

Tout est une seule étape d'annulation. La console et le message disent combien de displacements ont été mis à jour, combien de sommets ont bougé, combien ont été remodelés sur un nouveau maillage, créés ou ignorés (un displacement supprimé depuis l'envoi ou dont la puissance a changé est ignoré).

Si vous changez la map après l'envoi, les sommets que vous n'avez pas touchés dans Blender gardent l'état actuel de la map : seul ce que Blender a déplacé est écrit.

## Brushes et props

- **Brushes** : le maillage est transformé en brushes avec le constructeur de [Convertir le prop en brushes](propper.md) (forme « rendu ») : les brushes envoyés sont supprimés et les nouveaux prennent leur place (dans le même func_detail, ou groupés dans le monde) et sont sélectionnés. Un maillage qui ne peut pas devenir des brushes (plus de 20000 triangles, ou plus de 1024 morceaux) est compilé en modèle et placé comme `prop_static` à l'origine de la map, où il recouvre exactement les anciens brushes ; le lien suit ensuite ce prop. Après une annulation, la mise à jour suivante remplace les brushes qui existent à nouveau.
- **Props** : le modèle est recompilé (textures en VTF et VMT, collision, niveaux de détail, avec les options actuelles de l'[import de modèles](model-import.md) et les réglages enregistrés de LOD et de paramètres de matériaux) sous `models/hammerpro/<nom>.mdl`. Le nom est celui d'un modèle déjà dans `models/hammerpro`, ou le nom d'origine suivi de `_blender`. Le contenu du jeu est monté à nouveau, puis chaque prop lié reçoit ce modèle.

Noms de matériaux : Blender ajoute `.001` aux noms dupliqués, Hammer Pro retire ce suffixe. Un matériau créé dans Blender utilise son nom comme nom de matériau Source, nommez-le donc d'après un matériau Source pour avoir la bonne texture sur les brushes.

## Lancer Blender

Hammer Pro cherche Blender dans cet ordre : le paramètre `blender.path`, le registre Windows (l'association des fichiers `.blend` et les clés Blender Foundation), les dossiers standards (`Program Files\Blender Foundation\Blender x.y`, le dossier `Programs` par utilisateur, `/Applications`, `/opt`), une bibliothèque Steam, puis le `PATH`. La version la plus récente gagne. **Paramètres > Lien avec Blender** montre ce qui a été trouvé.

Il lance Blender avec un petit script Python généré (`startup.py` dans le dossier de travail) qui vide la scène par défaut, charge l'add-on, importe le fichier et cadre la vue. Rien n'est modifié dans vos réglages de Blender. Envoyer à nouveau pendant que le Blender lancé par Hammer Pro est encore ouvert n'en lance pas un autre : l'add-on y remarque le nouveau fichier en moins d'une seconde et remplace la scène par lui.

Quand Blender est introuvable, le fichier est quand même exporté et une fenêtre le dit, avec un bouton **Parcourir...** pour choisir `blender.exe` (ou `blender-launcher.exe`), et **Lancer Blender** pour réessayer. Le programme choisi est enregistré dans `blender.path`.

## L'add-on Hammer Pro Bridge

L'add-on est un fichier Python que Hammer Pro porte dans son programme et écrit à la demande. Il marche avec Blender 2.8 et suivants (vérifié avec la 5.2).

- **Aucune installation nécessaire** : il est chargé à chaque lancement de Blender par Hammer Pro.
- **Pour l'avoir dans toute session Blender** : **Outils > Blender et MCP > Installer l'add-on Blender** (ou **Paramètres > Lien avec Blender > Installer l'add-on Blender**, ou `blender_setup` avec `install_addon`). Il écrit `hammerpro_bridge.py` dans le dossier `scripts/addons` de chaque version de Blender installée trouvée dans votre dossier de configuration de Blender (`%APPDATA%\Blender Foundation\Blender\<version>\scripts\addons` sous Windows, `~/.config/blender/<version>/scripts/addons` sous Linux, `~/Library/Application Support/Blender/<version>/scripts/addons` sous macOS). Puis, dans Blender, ouvrez **Edit > Preferences > Add-ons** et activez **Hammer Pro Bridge** (dans Blender 4.2 et suivants il est listé parmi les add-ons historiques ; utilisez **Install from Disk** avec le fichier s'il n'est pas listé).
- Si aucun dossier de configuration de Blender n'existe encore, le fichier est écrit dans le dossier de travail et le message indique où, pour **Install from Disk**.

Dans Blender l'add-on donne :

- Un menu **Hammer Pro** dans la barre du haut et un onglet **Hammer Pro** dans la barre latérale de la vue 3D (touche `N`), avec le nom du fichier lié et **Update in Hammer Pro**, qui exporte la scène vers le fichier surveillé par Hammer Pro.
- **Export on save** (une case, cochée par défaut) : à chaque enregistrement du `.blend`, la scène est exportée de la même façon.
- Un minuteur qui charge un nouveau fichier quand Hammer Pro envoie à nouveau.

L'add-on trouve le fichier surveillé grâce à la variable d'environnement `HAMMERPRO_LINK` (définie quand Hammer Pro lance Blender) ou au fichier de lien `link.json` du dossier de travail. Après **Arrêter le lien direct avec Blender**, les boutons indiquent qu'il n'y a pas de lien.

## Lien direct

Après un envoi, Hammer Pro surveille le fichier exporté (paramètre `blender.live_link`, activé par défaut). Quand Blender l'écrit et que le fichier ne change plus pendant un court instant, Hammer Pro le lit en arrière-plan et l'applique. Avec le lien direct désactivé, utilisez **Mettre à jour depuis Blender** (ou `blender_pull`) quand vous voulez le changement. **Arrêter le lien direct avec Blender** met fin à la surveillance.

La surveillance ne coûte rien tant que le fichier ne change pas : un petit fil en arrière-plan regarde la date et la taille du fichier quatre fois par seconde et ne réveille l'éditeur que quand elles changent ; Hammer Pro au repos ne redessine donc pas pendant que le lien est actif.

## MCP

| Outil | Ce qu'il fait |
|---|---|
| `send_to_blender` | Comme **Envoyer vers Blender** : `format`, `launch`, `live_link`. Répond avec le fichier, ce qui est lié (`displacements`, `brushes` ou `prop`), et si Blender a démarré, était déjà ouvert ou est introuvable. |
| `blender_status` | Blender trouvé ou non, les paramètres, le dossier de travail, le lien actuel (fichier, format, genre, nombre d'éléments, mises à jour faites), le dernier message. |
| `blender_pull` | **Mettre à jour depuis Blender** tout de suite (une étape d'annulation) ; répond avec ce qui a été mis à jour. |
| `blender_setup` | `path` (vérifié : un programme ou dossier Blender), `format`, `live_link`, `detect`, `install_addon`, `stop_link`. |

Les paramètres `blender.format` et `blender.live_link` marchent aussi avec `set_setting` ; `blender.path` se change avec `blender_setup`, qui vérifie que le fichier est un programme Blender. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

Dans les exécutions automatiques (`--script`, `--screenshot`) Blender n'est jamais lancé et le dossier de travail est temporaire.

## Paramètres

Tout est dans **Paramètres > Lien avec Blender** et dans la table `[blender]` de `settings.toml`.

| Clé | Défaut | Effet |
| --- | --- | --- |
| `blender.path` | `""` | Le programme Blender (`blender.exe`, `blender-launcher.exe`, ou le dossier de Blender) ; vide veut dire que Hammer Pro cherche (voir « Lancer Blender »). **Parcourir...** le choisit, **Détecter** l'efface. |
| `blender.format` | `"gltf"` | Format de fichier d'un envoi : `gltf` (un `.glb`), `obj` ou `fbx`. |
| `blender.live_link` | `true` | Surveille le fichier exporté après un envoi et met la map à jour quand Blender l'écrit. |

Fichiers utilisés : le dossier de travail est `HammerPro/blender` dans le dossier de données locales de votre compte (définissez la variable d'environnement `HAMMERPRO_BLENDER_DIR` pour le déplacer) ; il contient le fichier exporté, les textures, `link.json`, `startup.py` et `hammerpro_bridge.py`. Les anciens exports y sont remplacés à chaque envoi.

## FAQ

**Rien ne se passe quand j'enregistre dans Blender.** Vérifiez que le lien est toujours actif (Paramètres > Lien avec Blender montre le fichier lié), que **Export on save** est coché dans l'onglet Hammer Pro, ou cliquez **Update in Hammer Pro**. Le fichier doit être celui du dossier de travail.

**La mise à jour dit qu'un displacement a été remodelé sur le nouveau maillage.** Ses sommets ou ses faces ont été ajoutés, supprimés ou réordonnés, donc la grille exacte n'a pas pu être lue. La forme est projetée à la place ; gardez la grille (déplacez des sommets, ne changez pas la topologie) pour un résultat exact.

**Un displacement n'a pas changé.** Il a été supprimé depuis l'envoi, sa puissance a changé (par exemple vous l'avez subdivisé), ou le déplacement était sous la tolérance. Renvoyez-le.

**Les raccords se sont ouverts.** Seuls les displacements envoyés sont recousus. Envoyez aussi les voisins, ou utilisez **Coudre** ensuite.

**Mon prop n'a pas changé dans la vue 3D.** Le contenu du jeu est monté à nouveau après la recompilation, ce qui prend un moment sur les grosses installations. La console dit « Mise à jour de Blender : ... reconstruit ».

**Les textures sont fausses sur mes brushes après une mise à jour.** Les matériaux qui portent le même nom que le matériau Source le gardent. Renommez-les dans Blender avec le chemin du matériau Source.

## Pages liées

- [Outil displacement](displacement-tool.md)
- [Import de modèles](model-import.md)
- [Propper](propper.md)
- [Outils BSP : Exporter en OBJ](bsp-tools.md#exporter-en-obj)
- [Serveur MCP](../reference/mcp-server.md)

---
[Retour à l'index de la documentation](../README.md)
