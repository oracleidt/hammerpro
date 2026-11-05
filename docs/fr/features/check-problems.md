[Documentation](../README.md) > Fonctionnalités > Vérifier les problèmes | [English](../../en/features/check-problems.md) | [Русский](../../ru/features/check-problems.md)

# Vérifier les problèmes

Hammer Pro vérifie la map en arrière-plan pendant que vous travaillez : brushes invalides, liens d'entités cassés, contenu manquant, limites du moteur, edicts et fuites. Les résultats s'affichent dans la barre d'état et dans la fenêtre **Vérifier les problèmes**, où la plupart des problèmes se corrigent en un clic. La fenêtre **Informations sur la map** donne les statistiques et les limites de la map, et quelques outils de plus vous aident à vous repérer : suggestions de func_detail, **Aller à** et **Comparer avec une autre map**.

## Vérification en direct et barre d'état

Peu après chaque modification, la map est vérifiée sur un thread de fond (paramètre **Vérifier la map pendant l'édition**, activé par défaut). La barre d'état affiche à droite :

| Segment | Contenu | Clic |
| --- | --- | --- |
| Totaux des problèmes | Croix rouge et nombre d'erreurs, signe d'avertissement et nombre d'avertissements, une coche quand tout va bien. `...` pendant une vérification. L'infobulle donne aussi le nombre de remarques. | Ouvre **Vérifier les problèmes**. |
| Badge de fuite | **Fuite** (rouge), seulement quand la vérification instantanée a trouvé un chemin d'une entité vers le vide. | Va à la fuite et trace son chemin (voir [Fuites et pointfiles](leaks-pointfiles.md)). |
| Jauge des limites | La limite du moteur la plus proche de son maximum, en pourcentage : vert, jaune dès 80 %, rouge au-delà de 100 %. L'infobulle nomme la limite et donne les edicts utilisés et restants. | Ouvre **Informations sur la map**. |

## La fenêtre Vérifier les problèmes

Ouvrez-la avec **Map > Vérifier les problèmes** (`Alt+P`) ou en cliquant les totaux de la barre d'état. Tant qu'elle est ouverte, la liste suit la vérification en direct.

| Contrôle | Effet |
| --- | --- |
| Boutons erreurs, avertissements et informations | Chacun affiche son total et masque ou affiche cette gravité (**Afficher les erreurs**, **Afficher les avertissements**, **Afficher les informations**). |
| **Ignorés (N)** | Présent quand vous ignorez des problèmes : les réaffiche (grisés) pour pouvoir ne plus les ignorer. |
| **Objets visibles seulement** | N'affiche que les problèmes des objets montrés dans les vues, comme « Vérifier les objets visibles seulement » de Hammer. Enregistré dans `health.visible_only` (désactivé par défaut). Les objets masqués restent comptés dans la barre d'état. |
| **Revérifier** | Lance la vérification tout de suite. |
| **Tout corriger (N)** | Corrige tous les problèmes corrigeables non ignorés, en une seule étape d'annulation. |

Chaque ligne affiche l'icône de gravité, la description (texte complet dans l'infobulle), le nombre d'objets ou de faces concernés, puis les boutons **Aller à** (sélectionne les objets et centre les vues dessus) et **Corriger** quand une correction automatique existe (l'infobulle dit ce qu'elle fait). Un clic ou un double-clic sur la ligne y va aussi. Clic droit sur une ligne pour **Aller à**, **Corriger**, **Ignorer** (masque ce problème pour cette map, mémorisé avec la map), **Ignorer tous les « type »**, **Ne plus ignorer** et **Copier le texte**.

Des messages en haut indiquent quand la map est en cours de vérification, quand la recherche de fuites tourne et quand le contenu du jeu est encore en chargement (les matériaux et modèles manquants peuvent ne pas encore être listés).

### Ce qui est vérifié

| Domaine | Problèmes |
| --- | --- |
| Brushes | solide invalide, brush minuscule (moins de 1 unité cube), plan en double, trop de faces (plus de 128), contenus mélangés (faces d'eau et faces solides sur un brush), axes de texture invalides, solide hors limites, brush d'origine dans le monde, texture d'outil sur le monde (textures qui ne marchent que sur les entités de brush) |
| Ids et références | id d'objet en double, id de face en double, ids renumérotés au chargement, groupe manquant, visgroup manquant |
| Entités | classe inconnue, cible manquante, cible de sortie manquante, entrée inconnue, entité dans un brush, entité hors limites, entité hors de la map (risque de fuite), entité de brush vide, entité unique en double, clé trop longue (31 et 1023 caractères), mauvais chemin de contenu (espaces ou caractères spéciaux), pas de point d'apparition, map vide |
| Contenu | matériau manquant, modèle manquant, son manquant, système de particules manquant, mauvais nom de matériau |
| Displacements et overlays | displacement cassé (face sans exactement 4 coins), displacement sur une entité de brush, faces d'overlay manquantes, overlay sur trop de faces (64 au maximum) |
| Éclairage et ciel | ciel sans soleil (faces de ciel mais pas de `light_environment`), trop de lumières nommées, plusieurs sky_camera, mauvaise échelle de sky_camera, changement de skybox sans sky_camera, skybox par joueur |
| Areaportals | brushes d'areaportal (un seul permis), areaportal sans porte, areaportal et porte désaccordés, areaportal à travers l'eau, areaportal à travers un occluder |
| Limites et fuites | limite proche, limite dépassée (la compilation échouera), budget d'edicts, fuite, vérification des fuites incomplète, candidat func_detail |

### Corrections automatiques

| Problème | Correction |
| --- | --- |
| solide invalide | Supprimer les solides invalides |
| id d'objet en double, id de face en double | Attribuer de nouveaux ids aux doublons |
| cible de sortie manquante | Supprimer les sorties cassées |
| entité de brush vide | Supprimer les entités de brush vides |
| displacement cassé | Retransformer les displacements cassés en faces simples |
| groupe manquant, visgroup manquant | Supprimer les références aux groupes manquants |
| axes de texture invalides | Réaligner les textures sur le monde |
| plan en double | Supprimer les faces en double |
| faces d'overlay manquantes | Retirer les faces inexistantes des listes (un overlay qui n'est plus sur aucune face est supprimé) |
| displacement sur une entité de brush | Déplacer les solides dans le monde |
| brush minuscule | Supprimer les solides minuscules |
| brush d'origine dans le monde | Supprimer les brushes d'origine du monde |
| candidat func_detail | Les lier à un func_detail (une seule étape d'annulation) |
| areaportal et porte désaccordés | Faire démarrer l'areaportal comme sa porte |

Chaque correction est une étape d'annulation : `Ctrl+Z` remet la map comme avant.

## Suggestions de func_detail

**Map > Suggestions de func_detail...** liste les brushes du monde qui pourraient être des `func_detail` : brushes petits ou complexes, éloignés de l'enveloppe de la map, vérifiés pour que la map reste étanche. La visibilité ignore les brushes de détail, la map compile donc plus vite. Les suggestions viennent de la vérification instantanée des fuites : la map doit être étanche (*La map fuit : les suggestions demandent une map étanche*, avec **Afficher la fuite**).

Chaque ligne (**Solide N**, son nombre de faces, *petit* ou *complexe*) sélectionne le brush et centre les vues dessus. Utilisez **Tout cocher**, **Tout décocher**, **Sélectionner les cochés**, puis **Convertir N en func_detail** pour lier les brushes cochés à un `func_detail` en une seule étape d'annulation. Le bouton **Liste** du problème « candidat func_detail » ouvre la même fenêtre.

## Informations sur la map

**Map > Informations sur la map** ouvre une fenêtre à deux onglets.

**Vue d'ensemble** : nombre de solides, de solides du monde, de faces, d'entités (ponctuelles et de brush), de displacements, de matériaux distincts, de groupes, de visgroups et de solides invalides ; les limites (min, max, taille) ; le maillage de navigation (zones, zones isolées) ; les sous-cartes de la map s'il y en a ; et **Matériaux utilisés (N)**, triés **Par usage** ou **Par nom** avec un filtre. Double-cliquez un matériau pour sélectionner les objets qui l'utilisent ; clic droit pour **Sélectionner les objets qui l'utilisent**, **Utiliser comme texture actuelle** et **Copier le texte**.

**Limites du moteur** (le nom de l'onglet passe en jaune ou en rouge quand une limite est proche) : chaque limite du moteur (brushes, faces de brushes, plans, sommets, arêtes, arêtes de surface, faces, nœuds BSP, infos de texture, matériaux, modèles de brushes, entités, overlays, displacements, cubemaps, lumières du monde, lightmaps et lightmaps HDR) avec sa **Limite**, l'**Estimation** mesurée sur la map pendant l'édition, et le nombre exact de la **Dernière compilation** lu dans la map compilée. On y trouve aussi les zones et les clusters de visibilité, les plus gros utilisateurs d'une limite (cliquez pour sélectionner), et trois sections :

- **Edicts (Garry's Mod)** : les entités qui existent encore quand la map tourne, comparées aux 8192 que gère Garry's Mod, avec des conseils (gardez-en environ 2000 libres pour ce que les joueurs font apparaître, `prop_dynamic` inutilisés, lumières nommées, `func_brush`/`func_wall`/`func_illusionary` sans nom).
- **Lightmaps** : les données d'éclairage estimées face à la limite du moteur, les faces les plus coûteuses, les faces à échelle de lightmap très fine, les faces que VBSP doit découper et les displacements trop larges pour VBSP.
- **Mémoire des textures** : cliquez **Calculer** pour additionner les fichiers de texture des matériaux utilisés (nécessite le contenu du jeu).

### Profils de limites

Les limites changent selon les versions de Source. **Limites de :** (dans la fenêtre Informations sur la map et dans **Paramètres > Santé de la map**) choisit les maximums :

| Profil | Pour |
| --- | --- |
| **Automatique** | Par défaut (`auto`) : Garry's Mod 64 bits pour les maps Garry's Mod, les limites de Valve pour les autres jeux. |
| **Source SDK 2013** | Les limites d'origine de Valve. |
| **Garry's Mod 32 bits** | La version 32 bits de Garry's Mod. |
| **Garry's Mod 64 bits** | La version 64 bits de Garry's Mod. |
| **VBSP++ (Tools++)** | Les maps compilées avec les outils de Hammer++. |

## Aller à un id ou une position

**Map > Aller à un id de brush ou d'entité...** (`Ctrl+Maj+G`) ouvre **Aller à**. Tapez un id d'objet (`1234`, `#1234`, `id 1234`) ou trois coordonnées (`0 0 512`, `(1, 2, 3)`, `[1 2 3]`) et cliquez **Aller**. Une ligne copiée d'un journal de compilation marche aussi : le brush qu'elle nomme, sinon les premières coordonnées entre parenthèses, sinon ses trois derniers nombres. Un id sélectionne le solide ou l'entité et centre les vues dessus (un objet masqué est seulement centré) ; des coordonnées centrent les vues.

## Comparer avec une autre map

**Map > Comparer avec une autre map...** demande un `.vmf` ou `.vmx` (une ancienne version, une sauvegarde automatique, une copie) et liste les différences dans la fenêtre **Comparer des maps**, les valeurs se lisant *autre map > cette map* :

- des compteurs qui servent aussi de filtres : **Seulement dans cette map**, **Seulement dans l'autre map**, **Modifiés** ;
- une case de filtre (classe, id ou changement) ;
- chaque ligne : **Entité** ou **Brush**, son id et ce qui a changé (classe, clés, sorties, brushes d'une entité, matériaux, faces déplacées, alignement des textures, displacement, données de l'éditeur), avec **Aller à** et **Sélectionner** ;
- **Sélectionner les objets listés** (les objets qui ne sont que dans l'autre map ne peuvent pas être sélectionnés).

## Paramètres de santé de la map

**Paramètres > Santé de la map** (`[health]` dans settings.toml) :

| Option | Défaut |
| --- | --- |
| **Vérifier la map pendant l'édition** | activé |
| **Chercher les fuites pendant l'édition** | activé |
| **Tracer le chemin de la fuite trouvée** | activé |
| **Vérifier les problèmes ne liste que les objets visibles** | désactivé |
| **Limites de :** | Automatique |
| Options des portails | voir [Fuites et pointfiles](leaks-pointfiles.md) |

## MCP

`check_problems`, `get_problems`, `fix_problems`, `recheck_map`, `get_map_limits`, `get_map_stats` et `find_hidden_faces` donnent les mêmes vérifications aux applications d'IA. Voir la [référence des outils MCP](../../MCP.md) (en anglais).

## Astuces

- Laissez la vérification en direct activée : la barre d'état vous signale une sortie cassée ou un modèle manquant dès que vous le créez.
- Ignorez un problème connu (une entité de test) plutôt que de couper la vérification : les problèmes ignorés sont gardés avec la map.
- Avant une grosse compilation, regardez la jauge des limites ; les nombres exacts après compilation sont dans la colonne **Dernière compilation**.

## FAQ

**Pourquoi l'estimation diffère-t-elle du nombre compilé ?** Le compilateur découpe des faces et ajoute des sommets et des nœuds. Certains nombres ne sont connus qu'après une compilation ; la colonne **Dernière compilation** les donne.

**Où sont gardés les problèmes ignorés ?** Avec la map, dans son fichier annexe à côté du VMF (voir [Compatibilité VMF](../getting-started/vmf-compatibility.md)).

**La vérification est lente sur une énorme map. Puis-je la couper ?** Oui, décochez **Vérifier la map pendant l'édition** dans **Paramètres > Santé de la map**. **Revérifier** la lance toujours à la demande.

## Pages liées

- [Fuites et pointfiles](leaks-pointfiles.md)
- [Limites et journaux](../compiler/limits-and-logs.md)
- [Erreurs courantes](../troubleshooting/common-errors.md)
- [Référence des paramètres](../reference/settings.md)

---
[Retour à l'index de la documentation](../README.md)
