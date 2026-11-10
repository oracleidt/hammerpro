[Documentation](../README.md) > Fonctionnalités > Dossiers de textures personnalisés | [English](../../en/features/custom-texture-folders.md) | [Русский](../../ru/features/custom-texture-folders.md)

# Dossiers de textures personnalisés

Un dossier de textures personnalisé est n'importe quel dossier de votre disque dont Hammer Pro affiche directement les textures dans le navigateur : fichiers VMT et VTF, ou simples images (PNG, JPG, TGA, BMP) qui deviennent des matériaux sans aucune conversion. À la compilation, les matériaux utilisés par la map sont convertis et livrés avec elle.

## Ajouter un dossier

- Dans l'onglet **Textures**, ouvrez **Dossiers personnalisés (n)** et cliquez sur **Ajouter un dossier...**.
- Ou **Paramètres > Jeu > Dossiers de ressources personnalisés > Ajouter un dossier...** (plusieurs dossiers d'un coup).
- Ou **Nouveau dossier...** dans la fenêtre **Importer des images**.

Ajouter un dossier déjà présent dans la liste le réactive au lieu de l'ajouter deux fois. Les dossiers sont enregistrés dans `custom_texture_dirs` de `[game]` dans `settings.toml` (chemin, préfixe, activé).

## Modèles dans les dossiers personnalisés

Activez **Charger les modèles (MDL)** pour un dossier dans **Textures > Dossiers personnalisés**, ou dans **Paramètres > Jeu > Dossiers de ressources personnalisés**. Cette option est activée par défaut, y compris avec les anciennes configurations. Le dossier affiche son nombre de modèles indexés. Ils apparaissent dans le navigateur **Modèles** et peuvent être placés comme props.

| Dossier sélectionné | Fichier sur disque | Chemin dans la map |
|---|---|---|
| Racine du pack | `pack/models/props/rock.mdl` | `models/props/rock.mdl` |
| Dossier `models` | `pack/models/props/rock.mdl` | `models/props/rock.mdl` |
| Sous-dossier de `models` | `pack/models/props/rock.mdl`, en choisissant `pack/models/props` | `models/props/rock.mdl` |
| Dossier d'export libre | `export/props/rock.mdl`, en choisissant `export` | `models/props/rock.mdl` |

Conservez les fichiers associés `.vvd`, `.vtx` (dont `.dx90.vtx`), `.phy` et `.ani` à côté du `.mdl`, selon le modèle. Les modèles ne sont pas renommés et le préfixe des matériaux ne leur est pas appliqué. Placez les VMT/VTF dans l'arborescence `materials` du pack ou ajoutez ce dossier séparément. Le compilateur et l'empaquetage utilisent la même source montée.

Désactivez **Charger les modèles (MDL)** pour conserver les matériaux du dossier tout en excluant ses modèles. **Actualiser** prend en compte les fichiers ajoutés ou modifiés. Désactiver ou retirer le dossier entier retire aussi ses modèles du montage, sans supprimer les fichiers.

Avec Team Mapping, chaque participant doit ajouter les mêmes ressources localement. Un dossier synchronisé par votre logiciel de synchronisation peut être monté sur chaque PC. Les chemins absolus peuvent différer, mais les chemins virtuels `models/...` et `materials/...` doivent correspondre. Team Mapping transmet les modifications de la map, pas les fichiers des modèles ou textures. Synchronisez tous les fichiers associés avant d'actualiser. Voir [Team Mapping](team-mapping.md).

## Comment les noms de matériaux sont formés

Hammer Pro lit le dossier selon ces règles :

| Dossier | Noms des matériaux | Champ Préfixe |
|---|---|---|
| Il contient un sous-dossier `materials` (dossier de type jeu) | Tirés de ce dossier `materials`, comme un dossier de jeu ; ses `models`, `sound`... sont montés aussi. | Affiche `materials/`, non utilisé. |
| Il se trouve lui-même dans une arborescence `.../materials/...` | La partie de son chemin après `materials/` (`C:/art/materials/custom/bricks` donne `custom/bricks/...`). | Affiché, non utilisé. |
| Tout autre dossier | `<préfixe>/<chemin dans le dossier>`. | Modifiable. |

Dans le dernier cas, le préfixe est, dans cet ordre :

1. Le préfixe que vous avez saisi.
2. Si le champ est vide (ou contient la suggestion) : le préfixe qu'attendent les VMT du dossier. Un pack tout prêt dont les VMT indiquent `$basetexture "mypack/brick01"` à côté de `brick01.vtf` est monté sous `materials/mypack/`, et ses matériaux fonctionnent comme en jeu. Le champ affiche alors ce préfixe avec la mention **détecté**.
3. Sinon la suggestion `custom/<nom du dossier>` (`C:/Art/My Bricks` donne `custom/my_bricks`).

Un préfixe saisi est nettoyé : ASCII en minuscules, barres obliques, accents retirés, espaces et symboles changés en `_`, pas de `materials/` au début. Appuyez sur `Entrée` (ou quittez le champ) pour l'appliquer. Videz le champ pour revenir au préfixe automatique.

## La liste des dossiers

Chaque dossier de **Dossiers personnalisés (n)** dans le navigateur de textures comporte :

| Commande | Effet |
|---|---|
| Case (**Utiliser ce dossier**) | Active ou désactive le dossier. |
| Nom | Le nom du dossier (survolez pour le chemin complet). |
| Compteur | **N matériau(x)**, **Désactivé**, ou **pas encore chargé** pendant le rechargement du contenu. |
| Bouton filtre (**Afficher uniquement ce dossier**) | Sélectionne la puce de source du dossier : la grille n'affiche que ses matériaux. |
| Bouton dossier (**Ouvrir dans l'explorateur de fichiers**) | Ouvre le dossier. |
| Bouton corbeille (**Retirer ce dossier de la liste (les fichiers sont conservés)**) | Retire le dossier de la liste. Rien n'est supprimé du disque. |
| **Préfixe** | Voir plus haut. |

Des avertissements apparaissent sous un dossier si besoin :

- **N matériau(x) utilisent des textures introuvables** : ces VMT désignent des textures introuvables. Un VMT qui indique `$basetexture "pack/brick"` doit être placé sous `materials/pack/` : vérifiez le préfixe. Survolez l'avertissement pour la liste.
- **N fichier(s) ont des noms avec espaces, accents ou symboles** : renommez-les avec des minuscules, des chiffres et des tirets bas, sinon les outils de compilation et le jeu risquent de ne pas les trouver. Les images sont renommées automatiquement.

Sous la liste :

| Bouton | Effet |
|---|---|
| **Ajouter un dossier...** | Ajoute un dossier de fichiers VMT/VTF ou d'images. |
| **Actualiser** | Recharge le contenu du jeu et les dossiers personnalisés, pour prendre en compte les fichiers nouveaux ou modifiés. |
| **Importer des images...** | Convertit des images en fichiers VTF et VMT dans un dossier personnalisé (voir plus bas). |

Les mêmes dossiers sont listés dans **Paramètres > Jeu > Dossiers de ressources personnalisés**, avec les colonnes **Activé**, **Chemin** et **Préfixe** et un bouton pour retirer.

## Des images comme matériaux

Dans tous les types de dossiers, une image PNG, JPG, JPEG, TGA ou BMP sous la racine des matériaux, sans VMT à côté, devient tout de suite un matériau :

- Hammer Pro lui crée un VMT en mémoire : shader `LightmappedGeneric`, `$basetexture` vers l'image, plus `$alphatest 1` si son alpha n'est que totalement opaque ou totalement transparent (grillages), ou `$translucent 1` si elle a de la transparence partielle (vitres).
- Son nom est rendu sûr pour les outils de compilation et le moteur : `Mur Brique Été.png` devient `mur_brique_ete`.
- Le jeu exige des côtés en puissances de deux : chaque côté est arrondi à la puissance de deux la plus proche (à égalité vers le haut, 4096 au plus). L'éditeur projette déjà la texture à cette taille de jeu. Le navigateur affiche **Redimensionnée dans le jeu** sur ces matériaux, et le journal de compilation les signale.

## Importer des images

**Importer des images...** demande des fichiers images, puis ouvre la fenêtre **Importer des images** :

| Option | Effet | Par défaut |
|---|---|---|
| Liste | Les images à convertir (**N image(s)**). | |
| **Destination** | Le dossier personnalisé qui reçoit les fichiers. **Nouveau dossier...** ajoute un dossier personnalisé. **Aucun dossier personnalisé** s'il n'y en a pas. | premier dossier |
| **Propriété de surface** | `$surfaceprop` écrit dans les VMT (**Aucun**, ou une valeur courante comme `concrete`, `brick`, `metal`...). | **Aucun** |
| **Générer les mipmaps** | Écrit des mipmaps dans les VTF. | activé |
| **Écraser les fichiers existants** | Remplace les fichiers VTF et VMT déjà présents (sinon ces images échouent). | désactivé |
| **Importer** | Lance l'importation (aussi `Entrée`). La ligne au-dessus indique **Les matériaux seront nommés préfixe/<nom de l'image> dans dossier**. | |
| **Éditeur de shaders...** | Ouvre l'[éditeur de shaders](shader-editor.md) sur la première image de la liste, pour la teinter, fabriquer une normal map ou mélanger des tuiles avant de l'écrire. | |
| **Annuler** | Ferme la fenêtre ; pendant la conversion, s'arrête après le fichier en cours. | |

La conversion tourne en arrière-plan avec une barre de progression (**Importation N sur M...**). Chaque image donne un `.vtf` (format choisi selon son contenu) et un `.vmt` avec la même règle d'alpha que plus haut. Les fichiers vont dans `materials/<préfixe suggéré>` d'un dossier de type jeu, ou à la racine de tout autre dossier, sous son préfixe. Le nom de l'image est nettoyé de la même façon ; si deux images donnent le même nom, les suivantes échouent. Le résultat s'affiche : **N image(s) importée(s), M en échec**.

## À la compilation

Le compilateur voit toujours les matériaux personnalisés. Pour le jeu :

- **Empaqueter le contenu personnalisé** (option de la fenêtre de compilation ; **Intégrer le contenu personnalisé au BSP** dans les paramètres) met dans le BSP les matériaux, modèles, sons, fichiers de particules (avec le manifeste `maps/<map>_particles.txt` dont le jeu a besoin pour les charger), les scripts de soundscapes et de sons propres à la map, le ciel et les autres fichiers personnalisés utilisés par la map ; les images sont converties en VTF (DXT1 ou DXT5 avec mipmaps) et leur VMT est écrit.
- Les compilations qui n'intègrent pas le contenu copient le même contenu personnalisé (matériaux, modèles, sons, fichiers de particules avec leur manifeste, scripts de la map) dans `garrysmod/addons/hammerpro_dev` (**Addon de développement**), que le jeu charge : un `prop_static` personnalisé est donc affiché et solide aussi dans une compilation Brouillon ou Normal.
- **Exporter les matériaux personnalisés** (**Exporter les matériaux personnalisés vers le jeu** dans les paramètres) les copie aussi dans le dossier du jeu (désactivé par défaut).

Voir [Fenêtre de compilation](../compiler/compile-window.md).

## Astuces

- Gardez un dossier par pack de textures et laissez le préfixe automatique : les packs tout prêts sont détectés grâce à leurs VMT.
- Nommez vos images en minuscules avec des tirets bas dès le départ : les noms de matériaux correspondront aux noms de fichiers.
- Utilisez des images en puissances de deux (256, 512, 1024...) pour éviter l'avertissement de redimensionnement.

## FAQ

**Les textures de mon pack s'affichent en damier violet et noir.** Leurs VMT pointent vers un chemin qui ne correspond pas au préfixe. Lisez l'avertissement **utilisent des textures introuvables** et corrigez le préfixe (ou videz-le pour qu'il soit détecté).

**J'ai ajouté des fichiers mais ils n'apparaissent pas.** Cliquez sur **Actualiser**.

**Retirer un dossier supprime-t-il mes fichiers ?** Non, il est seulement retiré de la liste.

**Les joueurs ont-ils besoin de mon dossier ?** Non : l'intégration au BSP y met les matériaux utilisés.

## Pages liées

- [Navigateur de textures](texture-browser.md)
- [Éditeur de matériaux](material-editor.md)
- [Configuration du jeu](../getting-started/game-setup.md)
- [Fenêtre de compilation](../compiler/compile-window.md)
- [Ressources de la map](map-assets.md)
- [Référence des paramètres](../reference/settings.md)
- [Index et analyse du contenu](content-index.md)
- [Référence des outils MCP](../../MCP.md) (en anglais : `import_images`, `list_asset_sources`)

---
[Retour à l'index de la documentation](../README.md)
